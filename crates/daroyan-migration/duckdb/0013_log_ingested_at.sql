UPDATE log_lines SET ingested_at = logged_at WHERE ingested_at IS NULL;
