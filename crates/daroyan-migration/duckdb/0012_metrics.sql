CREATE TABLE metric_series (
    series_id BIGINT NOT NULL,
    token_id VARCHAR NOT NULL,
    name VARCHAR NOT NULL,
    unit VARCHAR NOT NULL,
    description VARCHAR NOT NULL,
    kind VARCHAR NOT NULL,
    is_monotonic BOOLEAN NOT NULL,
    temporality VARCHAR,
    service VARCHAR NOT NULL,
    server_id VARCHAR,
    instance VARCHAR,
    attributes JSON NOT NULL,
    resource JSON,
    bounds DOUBLE[],
    first_seen BIGINT NOT NULL,
    last_seen BIGINT NOT NULL
);

CREATE TABLE metric_points (
    series_id BIGINT NOT NULL,
    taken_at BIGINT NOT NULL,
    start_at BIGINT,
    value DOUBLE,
    delta DOUBLE,
    count UBIGINT,
    sum DOUBLE,
    min DOUBLE,
    max DOUBLE,
    buckets UBIGINT[],
    count_delta UBIGINT,
    sum_delta DOUBLE,
    bucket_deltas UBIGINT[],
    trace_id VARCHAR,
    span_id VARCHAR
);

CREATE TABLE metric_rollups (
    series_id BIGINT NOT NULL,
    hour BIGINT NOT NULL,
    points UBIGINT NOT NULL,
    last DOUBLE,
    min DOUBLE,
    max DOUBLE,
    avg DOUBLE,
    delta DOUBLE,
    count_delta UBIGINT,
    sum_delta DOUBLE,
    bucket_deltas UBIGINT[]
)
;

CREATE TABLE metric_series_staging (
    series_id BIGINT NOT NULL,
    token_id VARCHAR NOT NULL,
    name VARCHAR NOT NULL,
    unit VARCHAR NOT NULL,
    description VARCHAR NOT NULL,
    kind VARCHAR NOT NULL,
    is_monotonic BOOLEAN NOT NULL,
    temporality VARCHAR,
    service VARCHAR NOT NULL,
    server_id VARCHAR,
    instance VARCHAR,
    attributes JSON NOT NULL,
    resource JSON,
    bounds VARCHAR,
    first_seen BIGINT NOT NULL,
    last_seen BIGINT NOT NULL
);

CREATE TABLE metric_points_staging (
    series_id BIGINT NOT NULL,
    taken_at BIGINT NOT NULL,
    start_at BIGINT,
    value DOUBLE,
    delta DOUBLE,
    count UBIGINT,
    sum DOUBLE,
    min DOUBLE,
    max DOUBLE,
    buckets VARCHAR,
    count_delta UBIGINT,
    sum_delta DOUBLE,
    bucket_deltas VARCHAR,
    trace_id VARCHAR,
    span_id VARCHAR
)
