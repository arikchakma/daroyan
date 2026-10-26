CREATE TABLE log_lines (
    server_id VARCHAR NOT NULL,
    logged_at BIGINT NOT NULL,
    service VARCHAR NOT NULL,
    stream VARCHAR NOT NULL,
    severity UTINYINT NOT NULL,
    message VARCHAR NOT NULL,
    attributes JSON,
    trace_id VARCHAR,
    span_id VARCHAR
);
