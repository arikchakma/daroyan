CREATE TABLE disk_states (
    server_id VARCHAR NOT NULL,
    taken_at BIGINT NOT NULL,
    mount VARCHAR NOT NULL,
    filesystem VARCHAR NOT NULL,
    disk_total UBIGINT NOT NULL,
    disk_used UBIGINT NOT NULL
)
