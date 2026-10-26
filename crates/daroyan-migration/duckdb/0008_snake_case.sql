UPDATE server_events SET kind = CASE kind
    WHEN 'agentStarted' THEN 'agent_started'
    WHEN 'processExited' THEN 'process_exited'
    WHEN 'processRestarted' THEN 'process_restarted'
    WHEN 'processRestartFailed' THEN 'process_restart_failed'
    WHEN 'processRestartGaveUp' THEN 'process_restart_gave_up'
    ELSE kind
END
