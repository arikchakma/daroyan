CREATE TABLE spans (
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
    received_at BIGINT NOT NULL
);

CREATE TABLE span_exceptions (
    trace_id VARCHAR NOT NULL,
    span_id VARCHAR NOT NULL,
    service VARCHAR NOT NULL,
    fingerprint VARCHAR NOT NULL,
    type VARCHAR NOT NULL,
    message VARCHAR NOT NULL,
    stacktrace VARCHAR USING COMPRESSION zstd,
    occurred_at BIGINT NOT NULL
);
