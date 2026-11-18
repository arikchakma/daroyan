ALTER TABLE log_lines ADD COLUMN ingested_at BIGINT;
ALTER TABLE log_lines ADD COLUMN http_status USMALLINT;
ALTER TABLE log_lines ADD COLUMN http_method VARCHAR;
ALTER TABLE log_lines ADD COLUMN url_path VARCHAR;
ALTER TABLE log_lines ADD COLUMN client_ip VARCHAR;

CREATE TABLE archive_files (
    path VARCHAR PRIMARY KEY,
    tenant VARCHAR NOT NULL,
    kind VARCHAR NOT NULL,
    min_logged_at BIGINT NOT NULL,
    max_logged_at BIGINT NOT NULL,
    rows BIGINT NOT NULL,
    bytes BIGINT NOT NULL,
    services VARCHAR[] NOT NULL,
    columns VARCHAR[] NOT NULL,
    created_at BIGINT NOT NULL,
    deleted_at BIGINT
);

CREATE TABLE archive_state (
    name VARCHAR PRIMARY KEY,
    value BIGINT NOT NULL
);
