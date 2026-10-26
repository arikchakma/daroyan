CREATE TABLE server_states (
    server_id VARCHAR NOT NULL,
    taken_at BIGINT NOT NULL,
    uptime UBIGINT NOT NULL,
    cpu FLOAT NOT NULL,
    load1 DOUBLE NOT NULL,
    load5 DOUBLE NOT NULL,
    load15 DOUBLE NOT NULL,
    mem_used UBIGINT NOT NULL,
    swap_used UBIGINT NOT NULL,
    process_count UINTEGER NOT NULL,
    tcp_conn_count UINTEGER NOT NULL,
    udp_conn_count UINTEGER NOT NULL,
    disk_total UBIGINT NOT NULL,
    disk_used UBIGINT NOT NULL,
    disk_read_speed UBIGINT NOT NULL,
    disk_write_speed UBIGINT NOT NULL,
    net_in_speed UBIGINT NOT NULL,
    net_out_speed UBIGINT NOT NULL,
    net_in_transfer UBIGINT NOT NULL,
    net_out_transfer UBIGINT NOT NULL
);

CREATE TABLE server_events (
    server_id VARCHAR NOT NULL,
    occurred_at BIGINT NOT NULL,
    kind VARCHAR NOT NULL,
    data JSON NOT NULL
);
