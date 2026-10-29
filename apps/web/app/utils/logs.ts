export const OTLP_SERVER_ID = 'otlp';
export const OTLP_SERVER_NAME = 'OpenTelemetry';

type LogOrigin = {
  service: string;
  stream: string;
  server_id: string;
  serverName: string;
};

export function getLogOrigin(line: LogOrigin) {
  if (line.server_id === OTLP_SERVER_ID) {
    return `${line.service} sent this over OpenTelemetry.`;
  }
  if (line.stream === 'otlp') {
    return `${line.service} on ${line.serverName} sent this over OpenTelemetry.`;
  }

  return `${line.service} wrote this to ${line.stream} on ${line.serverName}.`;
}
