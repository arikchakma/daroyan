CREATE TABLE uptime_results (
    check_id VARCHAR NOT NULL,
    checked_at BIGINT NOT NULL,
    status VARCHAR NOT NULL,
    code USMALLINT,
    response_ms UINTEGER,
    error VARCHAR,
    cert_expires_at BIGINT
);
