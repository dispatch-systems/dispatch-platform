use dispatch_core::{
    Error, Result,
    db::Store,
    ensure,
    foundation::config::Config,
    manifest::registry,
    server::operations::{self, Lock},
};
use std::{io::Read, path::Path};
pub async fn run() -> Result<()> {
    crate::install();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("serve");
    if command == "browseros-worker" {
        ensure(args.len() == 2, "invalid_browser_worker_arguments", 400)?;
        return dispatch_core::collection::browser::browseros::worker_main(&args[1]).await;
    }
    if command == "restore" {
        ensure(args.len() == 3, "usage_restore_backup_empty_target", 400)?;
        println!(
            "{}",
            operations::restore(Path::new(&args[1]), Path::new(&args[2]))?
        );
        return Ok(());
    }
    let config = Config::load()?;
    if command == "status" {
        println!("{}", operations::status(&config)?);
        return Ok(());
    }
    // Without the platform lock: each family of commands says why it needs none.
    let mut families = registry()
        .features
        .iter()
        .filter_map(|f| f.commands.as_ref());
    if let Some(family) = families.find(|family| command.starts_with(family.prefix)) {
        println!("{}", (family.run)(&config, &args)?);
        return Ok(());
    }
    let _lock = Lock::acquire(&config.root)?;
    match command {
        "serve" => {
            ensure(
                config.platform().join("accounts.sqlite").is_file(),
                "run_bootstrap_before_starting",
                503,
            )?;
            let state = dispatch_core::State::new(config.clone())?;
            state
                .run(|db| {
                    ensure(
                        operations::has_active_owner(db)?,
                        "run_bootstrap_before_starting",
                        503,
                    )
                })
                .await?;
            state
                .run(|db| {
                    db.recover_jobs(true)?;
                    operations::clean_browser_runs(&db.config)
                })
                .await?;
            let listener =
                tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, config.port)).await?;
            let (stop, receiver) = tokio::sync::watch::channel(false);
            let jobs = tokio::spawn(dispatch_core::supervise(
                dispatch_core::collection::jobs::start(state.clone(), receiver.clone()),
                stop.clone(),
            ));
            let mail_state = state.clone();
            let mail = tokio::spawn(dispatch_core::supervise(
                async move {
                    dispatch_core::server::mail::mailer(mail_state, receiver).await;
                    Ok(())
                },
                stop.clone(),
            ));
            dispatch_core::foundation::observability::event(
                "info",
                "core.started",
                serde_json::json!({"environment":config.environment,"port":config.port,"release":config.release}),
            );
            let sender = stop.clone();
            let mut shutdown_receiver = stop.subscribe();
            let shutdown = async move {
                let mut term =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("SIGTERM handler");
                tokio::select! {_=tokio::signal::ctrl_c()=>{},_=term.recv()=>{},_=dispatch_core::cancelled(&mut shutdown_receiver)=>{}};
                sender.send_replace(true);
            };
            let server = axum::serve(
                listener,
                dispatch_core::server::http::router(state.clone())
                    .into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(shutdown);
            let result = server.await;
            stop.send_replace(true);
            state.browsers.close().await;
            let jobs = jobs.await.map_err(|_| Error::new("scheduler_failed", 500));
            let mail = mail.await.map_err(|_| Error::new("mailer_failed", 500));
            jobs??;
            mail??;
            result?;
        }
        "bootstrap" => {
            ensure(args.len() == 4, "usage_bootstrap_email_first_last", 400)?;
            ensure(
                unsafe { libc::isatty(libc::STDIN_FILENO) } == 0,
                "password_required_on_stdin",
                400,
            )?;
            let mut password = String::new();
            std::io::stdin().take(1024).read_to_string(&mut password)?;
            let db = Store::initialize(config)?;
            println!(
                "{}",
                operations::bootstrap(
                    &db,
                    &args[1],
                    &args[2],
                    &args[3],
                    password.trim_end_matches(['\r', '\n'])
                )?
            );
        }
        "seed" => operations::seed(&Store::initialize(config)?)?,
        "seed-agents" => println!(
            "{}",
            dispatch_core::mcp::synthetic::seed(&Store::initialize(config)?)?
        ),
        "backup" => {
            ensure(args.len() == 2, "usage_backup_destination", 400)?;
            println!("{}", operations::backup(&config, Path::new(&args[1]))?);
        }
        _ => {
            return Err(Error::new(
                "usage_serve_bootstrap_seed_status_backup_restore",
                400,
            ));
        }
    }
    Ok(())
}
