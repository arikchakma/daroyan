CREATE TABLE log_lines_packed (
    server_id VARCHAR NOT NULL,
    logged_at BIGINT NOT NULL,
    service VARCHAR NOT NULL,
    stream VARCHAR NOT NULL,
    severity UTINYINT NOT NULL,
    message VARCHAR NOT NULL,
    attributes JSON USING COMPRESSION zstd,
    trace_id VARCHAR,
    span_id VARCHAR,
    raw VARCHAR USING COMPRESSION zstd,
    resource JSON
);

INSERT INTO log_lines_packed SELECT * FROM log_lines;

DROP TABLE log_lines;

ALTER TABLE log_lines_packed RENAME TO log_lines;

CREATE TABLE spans_packed (
    trace_id VARCHAR NOT NULL,
    span_id VARCHAR NOT NULL,
    parent_span_id VARCHAR,
    service VARCHAR NOT NULL,
    name VARCHAR NOT NULL,
    kind VARCHAR NOT NULL,
    started_at BIGINT NOT NULL,
    duration_us BIGINT NOT NULL,
    status VARCHAR NOT NULL,
    status_message VARCHAR,
    http_method VARCHAR,
    http_route VARCHAR,
    http_status INTEGER,
    attributes JSON USING COMPRESSION zstd,
    resource JSON,
    events JSON USING COMPRESSION zstd,
    received_at BIGINT NOT NULL,
    server_id VARCHAR
);

INSERT INTO spans_packed SELECT * FROM spans;

DROP TABLE spans;

ALTER TABLE spans_packed RENAME TO spans;

CREATE TABLE span_exceptions_packed (
    trace_id VARCHAR NOT NULL,
    span_id VARCHAR NOT NULL,
    service VARCHAR NOT NULL,
    fingerprint VARCHAR NOT NULL,
    type VARCHAR NOT NULL,
    message VARCHAR NOT NULL,
    stacktrace VARCHAR USING COMPRESSION zstd,
    occurred_at BIGINT NOT NULL
);

INSERT INTO span_exceptions_packed SELECT * FROM span_exceptions;

DROP TABLE span_exceptions;

ALTER TABLE span_exceptions_packed RENAME TO span_exceptions
