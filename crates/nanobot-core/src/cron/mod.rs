pub mod service;
pub mod types;

pub use service::{compute_next_run, AddJobParams, CronService, CronServiceStatus, OnJobCallback};
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_cron_schedule_serialization() {
        let schedule = CronSchedule::every(5000);
        let json = serde_json::to_string(&schedule).unwrap();
        let parsed: CronSchedule = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.every_ms, Some(5000));
    }

    #[test]
    fn test_cron_schedule_at_serialization() {
        let schedule = CronSchedule::at(1700000000000);
        let json = serde_json::to_string(&schedule).unwrap();
        let parsed: CronSchedule = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.kind, ScheduleKind::At);
        assert_eq!(parsed.at_ms, Some(1700000000000));
    }

    #[test]
    fn test_cron_schedule_cron_expr_serialization() {
        let schedule = CronSchedule::cron_expr("0 0 * * * *", Some("UTC"));
        let json = serde_json::to_string(&schedule).unwrap();
        let parsed: CronSchedule = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.kind, ScheduleKind::Cron);
        assert_eq!(parsed.expr.as_deref(), Some("0 0 * * * *"));
        assert_eq!(parsed.tz.as_deref(), Some("UTC"));
    }

    #[test]
    fn test_cron_job_serialization_camel_case() {
        let job = CronJob {
            id: "test-id".to_string(),
            name: "test".to_string(),
            enabled: true,
            schedule: CronSchedule::every(60000),
            payload: CronPayload::default(),
            state: CronJobState {
                next_run_at_ms: Some(1000),
                last_run_at_ms: None,
                last_status: None,
                last_error: None,
            },
            created_at_ms: 500,
            updated_at_ms: 500,
            delete_after_run: false,
        };
        let json = serde_json::to_string(&job).unwrap();
        // Verify camelCase field names
        assert!(json.contains("createdAtMs"));
        assert!(json.contains("updatedAtMs"));
        assert!(json.contains("deleteAfterRun"));
        assert!(json.contains("nextRunAtMs"));
    }

    #[test]
    fn test_cron_store_default() {
        let store = CronStore::default();
        assert_eq!(store.version, 1);
        assert!(store.jobs.is_empty());
    }

    #[test]
    fn test_compute_next_run_every() {
        let schedule = CronSchedule::every(5000);
        let now = 1000;
        let next = compute_next_run(&schedule, now);
        assert_eq!(next, Some(6000));
    }

    #[test]
    fn test_compute_next_run_at_future() {
        let schedule = CronSchedule::at(2000);
        let next = compute_next_run(&schedule, 1000);
        assert_eq!(next, Some(2000));
    }

    #[test]
    fn test_compute_next_run_at_past() {
        let schedule = CronSchedule::at(500);
        let next = compute_next_run(&schedule, 1000);
        assert_eq!(next, None);
    }

    #[test]
    fn test_compute_next_run_cron_expr() {
        // "every second" cron expression
        let schedule = CronSchedule::cron_expr("* * * * * *", None);
        let now = chrono::Utc::now().timestamp_millis();
        let next = compute_next_run(&schedule, now);
        assert!(next.is_some());
        assert!(next.unwrap() > now);
    }

    #[test]
    fn test_compute_next_run_invalid_cron() {
        let schedule = CronSchedule::cron_expr("not a cron", None);
        let next = compute_next_run(&schedule, 1000);
        assert_eq!(next, None);
    }

    #[tokio::test]
    async fn test_add_and_list_jobs() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("jobs.json");
        let mut service = CronService::new(path);

        service
            .add_job("test", CronSchedule::every(60000), "hello", false, None, None)
            .unwrap();
        let jobs = service.list_jobs(false);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].name, "test");
    }

    #[tokio::test]
    async fn test_remove_job() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("jobs.json");
        let mut service = CronService::new(path);

        let job = service
            .add_job("test", CronSchedule::every(60000), "hello", false, None, None)
            .unwrap();
        assert!(service.remove_job(&job.id));
        assert_eq!(service.list_jobs(true).len(), 0);
    }

    #[tokio::test]
    async fn test_enable_disable_job() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("jobs.json");
        let mut service = CronService::new(path);

        let job = service
            .add_job("test", CronSchedule::every(60000), "hello", false, None, None)
            .unwrap();

        // Disable the job
        assert!(service.enable_job(&job.id, false));
        let jobs = service.list_jobs(false);
        assert_eq!(jobs.len(), 0); // disabled jobs excluded
        let all_jobs = service.list_jobs(true);
        assert_eq!(all_jobs.len(), 1);
        assert!(!all_jobs[0].enabled);

        // Re-enable
        assert!(service.enable_job(&job.id, true));
        let jobs = service.list_jobs(false);
        assert_eq!(jobs.len(), 1);
        assert!(jobs[0].enabled);
    }

    #[tokio::test]
    async fn test_persistence() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("jobs.json");

        // Create service and add job
        {
            let mut service = CronService::new(path.clone());
            service
                .add_job("persist-test", CronSchedule::every(30000), "msg", true, None, None)
                .unwrap();
        }

        // Create new service instance, load from disk
        {
            let mut service = CronService::new(path);
            service.start().unwrap();
            let jobs = service.list_jobs(true);
            assert_eq!(jobs.len(), 1);
            assert_eq!(jobs[0].name, "persist-test");
            service.stop();
        }
    }

    #[tokio::test]
    async fn test_service_status() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("jobs.json");
        let mut service = CronService::new(path);

        let status = service.status();
        assert!(!status.running);
        assert_eq!(status.job_count, 0);

        service
            .add_job("s1", CronSchedule::every(10000), "msg", false, None, None)
            .unwrap();
        let status = service.status();
        assert_eq!(status.job_count, 1);
        assert_eq!(status.enabled_count, 1);
        assert!(status.next_fire_at_ms.is_some());
    }

    #[tokio::test]
    async fn test_remove_nonexistent_job() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("jobs.json");
        let mut service = CronService::new(path);
        assert!(!service.remove_job("nonexistent-id"));
    }

    #[tokio::test]
    async fn test_enable_nonexistent_job() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("jobs.json");
        let mut service = CronService::new(path);
        assert!(!service.enable_job("nonexistent-id", true));
    }

    #[tokio::test]
    async fn test_add_job_with_channel() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("jobs.json");
        let mut service = CronService::new(path);

        let job = service
            .add_job(
                "channel-test",
                CronSchedule::every(60000),
                "hello",
                true,
                Some("telegram"),
                Some("user123"),
            )
            .unwrap();
        assert_eq!(job.payload.channel.as_deref(), Some("telegram"));
        assert_eq!(job.payload.to.as_deref(), Some("user123"));
        assert!(job.payload.deliver);
    }

    #[tokio::test]
    async fn test_add_job_delete_after_run() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("jobs.json");
        let mut service = CronService::new(path);

        let job = service
            .add_job_ext(AddJobParams {
                name: "one-shot",
                schedule: CronSchedule::at(chrono::Utc::now().timestamp_millis() + 100_000),
                message: "fire once",
                deliver: false,
                channel: None,
                to: None,
                delete_after_run: true,
            })
            .unwrap();
        assert!(job.delete_after_run);
    }
}
