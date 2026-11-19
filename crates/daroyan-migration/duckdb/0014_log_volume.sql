CREATE TABLE log_volume (
    minute BIGINT NOT NULL,
    server_id VARCHAR NOT NULL,
    service VARCHAR NOT NULL,
    severity UTINYINT NOT NULL,
    lines BIGINT NOT NULL,
    PRIMARY KEY (minute, server_id, service, severity)
);

INSERT INTO log_volume
SELECT logged_at // 60000 * 60000, server_id, service, severity, count(*)
FROM log_lines
GROUP BY ALL;
