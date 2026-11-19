use resonance_core::jobs::{Job, JobStatus, JobTier, JobsSnapshot, NewJob};
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::search::now_unix_ns;
use crate::{Database, DbError};

const JOB_COLUMNS: &str = "id, kind, tier, status, label, payload_json, done_items, total_items,
     error_message, created_ns, started_ns, finished_ns, paused";

fn read_job_row(row: &Row<'_>) -> Result<Job, DbError> {
    let raw_tier: i64 = row.get(2)?;
    let tier = JobTier::from_raw(raw_tier).ok_or_else(|| DbError::UnknownName {
        column: "job.tier",
        name: raw_tier.to_string(),
    })?;
    let status_name: String = row.get(3)?;
    let status = JobStatus::from_name(&status_name).ok_or(DbError::UnknownName {
        column: "job.status",
        name: status_name,
    })?;
    Ok(Job {
        id: row.get(0)?,
        kind: row.get(1)?,
        tier,
        status,
        label: row.get(4)?,
        payload_json: row.get(5)?,
        done_items: row.get(6)?,
        total_items: row.get(7)?,
        error_message: row.get(8)?,
        created_ns: row.get(9)?,
        started_ns: row.get(10)?,
        finished_ns: row.get(11)?,
        paused: row.get(12)?,
    })
}

fn job_by_id_in(connection: &Connection, job_id: i64) -> Result<Option<Job>, DbError> {
    let mut stmt = connection.prepare(&format!("SELECT {JOB_COLUMNS} FROM job WHERE id = ?1"))?;
    let mut rows = stmt.query([job_id])?;
    match rows.next()? {
        Some(row) => Ok(Some(read_job_row(row)?)),
        None => Ok(None),
    }
}

impl Database {
    /// Resets any jobs left in `running` when the previous process exited or crashed so they
    /// resume automatically from `queued`.
    pub fn recover_interrupted_jobs(&mut self) -> Result<usize, DbError> {
        Ok(self.connection.execute(
            "UPDATE job SET status = 'queued', started_ns = NULL WHERE status = 'running'",
            [],
        )?)
    }

    pub fn job_by_id(&self, job_id: i64) -> Result<Option<Job>, DbError> {
        job_by_id_in(&self.connection, job_id)
    }

    /// Inserts a new `queued` job unconditionally.
    pub fn enqueue_job(&mut self, new_job: &NewJob) -> Result<Job, DbError> {
        let created_ns = now_unix_ns();
        self.connection.execute(
            "INSERT INTO job (kind, tier, status, label, payload_json, done_items, total_items, created_ns)
             VALUES (?1, ?2, 'queued', ?3, ?4, 0, ?5, ?6)",
            params![
                new_job.kind,
                new_job.tier.raw(),
                new_job.label,
                new_job.payload_json,
                new_job.total_items,
                created_ns,
            ],
        )?;
        let id = self.connection.last_insert_rowid();
        job_by_id_in(&self.connection, id)?.ok_or_else(|| DbError::UnknownName {
            column: "job.id",
            name: id.to_string(),
        })
    }

    /// Returns an existing `queued` job with the same `(kind, payload_json)` when one is already
    /// waiting to start, or inserts a new `queued` job and returns `(job, true)`.
    pub fn enqueue_or_coalesce_job(&mut self, new_job: &NewJob) -> Result<(Job, bool), DbError> {
        let tx = self.connection.transaction()?;
        let existing_id: Option<i64> = tx
            .query_row(
                "SELECT id FROM job
                 WHERE status = 'queued' AND kind = ?1 AND payload_json = ?2
                 ORDER BY id ASC
                 LIMIT 1",
                params![new_job.kind, new_job.payload_json],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(id) = existing_id {
            if let Some(total) = new_job.total_items {
                tx.execute(
                    "UPDATE job SET total_items = ?2 WHERE id = ?1 AND status = 'queued'",
                    params![id, total],
                )?;
            }
            let job = job_by_id_in(&tx, id)?.ok_or_else(|| DbError::UnknownName {
                column: "job.id",
                name: id.to_string(),
            })?;
            tx.commit()?;
            return Ok((job, false));
        }

        let created_ns = now_unix_ns();
        tx.execute(
            "INSERT INTO job (kind, tier, status, label, payload_json, done_items, total_items, created_ns)
             VALUES (?1, ?2, 'queued', ?3, ?4, 0, ?5, ?6)",
            params![
                new_job.kind,
                new_job.tier.raw(),
                new_job.label,
                new_job.payload_json,
                new_job.total_items,
                created_ns,
            ],
        )?;
        let id = tx.last_insert_rowid();
        let job = job_by_id_in(&tx, id)?.ok_or_else(|| DbError::UnknownName {
            column: "job.id",
            name: id.to_string(),
        })?;
        tx.commit()?;
        Ok((job, true))
    }

    /// Claims the next queued job for either the interactive lane (`tier IN (0, 3)`) or the
    /// background lane (`tier IN (1, 2)`).
    pub fn claim_next_job(&mut self, interactive: bool) -> Result<Option<Job>, DbError> {
        let tx = self.connection.transaction()?;
        let sql = if interactive {
            "SELECT id FROM job
             WHERE status = 'queued' AND paused = 0 AND tier IN (0, 3)
             ORDER BY CASE tier WHEN 3 THEN 0 ELSE 1 END, id ASC
             LIMIT 1"
        } else {
            "SELECT id FROM job
             WHERE status = 'queued' AND paused = 0 AND tier IN (1, 2)
             ORDER BY tier ASC, id ASC
             LIMIT 1"
        };
        let next_id: Option<i64> = tx.query_row(sql, [], |row| row.get(0)).optional()?;
        let Some(id) = next_id else {
            return Ok(None);
        };
        let started_ns = now_unix_ns();
        tx.execute(
            "UPDATE job
             SET status = 'running',
                 started_ns = ?2,
                 finished_ns = NULL,
                 done_items = 0,
                 error_message = NULL
             WHERE id = ?1 AND status = 'queued'",
            params![id, started_ns],
        )?;
        let claimed = job_by_id_in(&tx, id)?;
        tx.commit()?;
        Ok(claimed)
    }

    /// True while any `Scan` (`0`) or `OnDemand` (`3`) job is queued or running and not paused.
    pub fn has_active_interactive_jobs(&self) -> Result<bool, DbError> {
        let found: Option<i64> = self
            .connection
            .query_row(
                "SELECT 1 FROM job WHERE status IN ('queued', 'running') AND paused = 0 AND tier IN (0, 3)
                 LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }

    /// True while any job in the background lane (`1` or `2`) is queued and not paused.
    pub fn has_queued_background_jobs(&self) -> Result<bool, DbError> {
        let found: Option<i64> = self
            .connection
            .query_row(
                "SELECT 1 FROM job WHERE status = 'queued' AND paused = 0 AND tier IN (1, 2) LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }

    /// Pauses or resumes a queued or running job. `None` when the job has finished or is gone.
    pub fn set_job_paused(&mut self, job_id: i64, paused: bool) -> Result<Option<Job>, DbError> {
        let changed = self.connection.execute(
            "UPDATE job SET paused = ?2 WHERE id = ?1 AND status IN ('queued', 'running')",
            params![job_id, paused],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        job_by_id_in(&self.connection, job_id)
    }

    pub fn is_job_paused(&self, job_id: i64) -> Result<bool, DbError> {
        Ok(self
            .connection
            .query_row("SELECT paused FROM job WHERE id = ?1", [job_id], |row| {
                row.get(0)
            })
            .optional()?
            .unwrap_or(false))
    }

    pub fn update_job_progress(
        &mut self,
        job_id: i64,
        done_items: u64,
        total_items: Option<u64>,
    ) -> Result<bool, DbError> {
        let updated = self.connection.execute(
            "UPDATE job
             SET done_items = ?2, total_items = ?3
             WHERE id = ?1 AND status = 'running'",
            params![job_id, done_items, total_items],
        )?;
        Ok(updated > 0)
    }

    pub fn complete_job(
        &mut self,
        job_id: i64,
        done_items: u64,
        total_items: Option<u64>,
    ) -> Result<Option<Job>, DbError> {
        let finished_ns = now_unix_ns();
        let changed = self.connection.execute(
            "UPDATE job
             SET status = 'completed',
                 done_items = ?2,
                 total_items = ?3,
                 error_message = NULL,
                 finished_ns = ?4
             WHERE id = ?1 AND status = 'running'",
            params![job_id, done_items, total_items, finished_ns],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        job_by_id_in(&self.connection, job_id)
    }

    pub fn fail_job(&mut self, job_id: i64, error_message: &str) -> Result<Option<Job>, DbError> {
        let finished_ns = now_unix_ns();
        let changed = self.connection.execute(
            "UPDATE job
             SET status = 'failed',
                 error_message = ?2,
                 finished_ns = ?3
             WHERE id = ?1 AND status = 'running'",
            params![job_id, error_message, finished_ns],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        job_by_id_in(&self.connection, job_id)
    }

    pub fn cancel_queued_job(&mut self, job_id: i64) -> Result<Option<Job>, DbError> {
        let finished_ns = now_unix_ns();
        let changed = self.connection.execute(
            "UPDATE job
             SET status = 'cancelled',
                 finished_ns = ?2
             WHERE id = ?1 AND status = 'queued'",
            params![job_id, finished_ns],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        job_by_id_in(&self.connection, job_id)
    }

    pub fn mark_running_job_cancelled(&mut self, job_id: i64) -> Result<Option<Job>, DbError> {
        let finished_ns = now_unix_ns();
        let changed = self.connection.execute(
            "UPDATE job
             SET status = 'cancelled',
                 finished_ns = ?2
             WHERE id = ?1 AND status = 'running'",
            params![job_id, finished_ns],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        job_by_id_in(&self.connection, job_id)
    }

    pub fn retry_job(&mut self, job_id: i64) -> Result<Option<Job>, DbError> {
        let changed = self.connection.execute(
            "UPDATE job
             SET status = 'queued',
                 paused = 0,
                 done_items = 0,
                 error_message = NULL,
                 started_ns = NULL,
                 finished_ns = NULL
             WHERE id = ?1 AND status IN ('failed', 'cancelled')",
            [job_id],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        job_by_id_in(&self.connection, job_id)
    }

    pub fn clear_finished_jobs(&mut self) -> Result<usize, DbError> {
        Ok(self.connection.execute(
            "DELETE FROM job WHERE status IN ('completed', 'failed', 'cancelled')",
            [],
        )?)
    }

    pub fn jobs_snapshot(&self, recent_limit: u32) -> Result<JobsSnapshot, DbError> {
        let mut active_stmt = self.connection.prepare(&format!(
            "SELECT {JOB_COLUMNS}
             FROM job
             WHERE status IN ('running', 'queued')
             ORDER BY CASE status WHEN 'running' THEN 0 ELSE 1 END,
                      CASE tier WHEN 3 THEN 0 WHEN 0 THEN 1 WHEN 1 THEN 2 ELSE 3 END,
                      id ASC"
        ))?;
        let mut active = Vec::new();
        let mut active_rows = active_stmt.query([])?;
        while let Some(row) = active_rows.next()? {
            active.push(read_job_row(row)?);
        }

        let mut recent_stmt = self.connection.prepare(&format!(
            "SELECT {JOB_COLUMNS}
             FROM job
             WHERE status IN ('completed', 'failed', 'cancelled')
             ORDER BY COALESCE(finished_ns, created_ns) DESC, id DESC
             LIMIT ?1"
        ))?;
        let mut recent = Vec::new();
        let mut recent_rows = recent_stmt.query([recent_limit])?;
        while let Some(row) = recent_rows.next()? {
            recent.push(read_job_row(row)?);
        }

        Ok(JobsSnapshot { active, recent })
    }
}
