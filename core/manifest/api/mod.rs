//! Everyone a DSP's features name, as the directory tells them apart.
pub(crate) mod routes;

/// Its routes, each with its path and access, which `tests/api/routes.txt` lists.
pub fn routes() -> Vec<crate::server::http::Route> {
    routes::routes()
}
