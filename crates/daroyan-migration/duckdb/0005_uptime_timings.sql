ALTER TABLE uptime_results ADD COLUMN dns_ms UINTEGER;
ALTER TABLE uptime_results ADD COLUMN connect_ms UINTEGER;
ALTER TABLE uptime_results ADD COLUMN tls_ms UINTEGER;
ALTER TABLE uptime_results ADD COLUMN ttfb_ms UINTEGER;
