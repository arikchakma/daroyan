CREATE TABLE cron_runs (
    cron_id VARCHAR NOT NULL,
    scheduled_at BIGINT,
    started_at BIGINT,
    finished_at BIGINT NOT NULL,
    status VARCHAR NOT NULL,
    exit_code USMALLINT,
    duration_ms UINTEGER,
    output VARCHAR
);
