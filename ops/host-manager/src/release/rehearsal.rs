//! Startup on a second restored copy on physical Dev; no Production test execution.
use super::*;
use crate::releases::Provenance;

const REHEARSED: &str = "upgrade-rehearsal.json";

impl Release<'_> {
    fn rehearsal_binding(&self, prepared: &Provenance, backup: &backup::Backup) -> Value {
        json!({"format":2,"executor":"dispatch-dev","version":self.version,
            "commit":prepared.commit,"runtimeDigest":prepared.runtime_digest,"archiveSha256":prepared.archive_sha256,
            "backup":backup.destination,"proof":backup.proof})
    }
    /// Compare a newly exported inventory, including original capture/config identity.
    fn same_backup(&self, saved: &local::CopyProof, exported: &local::CopyProof) -> Result<()> {
        require(
            saved.content_sha256 == exported.content_sha256
                && saved.config_sha256 == exported.config_sha256
                && saved.taken_at == exported.taken_at
                && saved.files == exported.files,
            "Backup data or startup config changed after Dev verification",
        )?;
        backup::fresh(self.system.now(), saved.taken_at)
    }
    pub(super) fn rehearse(&self, prepared: &Provenance) -> Result<bool> {
        if self.new_migrations(&prepared.commit)?.is_empty() {
            return Ok(false);
        }
        let backup = self.checked_backup(prepared)?;
        let receipt = self.output.join(REHEARSED);
        let binding = self.rehearsal_binding(prepared, &backup);
        if receipt.try_exists()? && io::read_json(&receipt)?["binding"] == binding {
            let inventory =
                self.dev_copy(&backup.destination, prepared, local::Check::Inventory)?;
            self.same_backup(&backup.proof, &inventory)?;
            return Ok(false);
        }
        let proof = self.dev_copy(&backup.destination, prepared, local::Check::Upgrade)?;
        self.same_backup(&backup.proof, &proof)?;
        let report = proof.report.ok_or("Dev startup reported nothing")?;
        io::write_json(
            &receipt,
            &json!({"binding":binding,"migrations":report["migrations"],
            "databases":report["databases"],"errors":report["errors"]}),
        )?;
        Ok(true)
    }
    fn matching_rehearsal(&self, prepared: &Provenance) -> Result<backup::Backup> {
        let backup = self.checked_backup(prepared)?;
        let saved = io::read_json(&self.output.join(REHEARSED))?;
        require(
            saved["binding"] == self.rehearsal_binding(prepared, &backup),
            "No matching Dev startup proof",
        )?;
        Ok(backup)
    }
    /// Finish the potentially long export before the final Production readiness check.
    pub(super) fn revalidate_rehearsal(&self, prepared: &Provenance) -> Result<()> {
        if self.new_migrations(&prepared.commit)?.is_empty() {
            return Ok(());
        }
        let backup = self.matching_rehearsal(prepared)?;
        let inventory = self.dev_copy(&backup.destination, prepared, local::Check::Inventory)?;
        self.same_backup(&backup.proof, &inventory)
    }
    /// Only local reads after readiness: preserve the original capture deadline even when
    /// readiness waited, and bind the live startup config observed by that final check.
    pub(super) fn ready_rehearsal(&self, prepared: &Provenance) -> Result<()> {
        let backup = self.matching_rehearsal(prepared)?;
        let ready = io::read_json(&self.output.join(production::CHECKED))?;
        require(
            ready["startupConfigSha256"] == backup.proof.config_sha256,
            "Production startup config changed after Dev rehearsal",
        )
    }
    pub(super) fn rehearsal(&self) -> Result<Option<Value>> {
        let receipt = self.output.join(REHEARSED);
        receipt
            .try_exists()?
            .then(|| io::read_json(&receipt))
            .transpose()
    }
}
