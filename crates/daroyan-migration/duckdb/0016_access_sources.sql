CREATE TABLE access_sources (
    server_id VARCHAR NOT NULL,
    ip VARCHAR NOT NULL,
    first_seen BIGINT NOT NULL,
    last_seen BIGINT NOT NULL,
    last_user VARCHAR NOT NULL,
    PRIMARY KEY (server_id, ip)
);
