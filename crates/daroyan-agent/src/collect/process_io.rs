use std::collections::HashMap;
use std::io::{ErrorKind, Read};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use log::warn;
use sysinfo::Process;

pub const SOCKET_PATH: &str = "/run/daroyan-io/io.sock";
const READ_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_BYTES: u64 = 8 * 1024 * 1024;

pub type IoTotals = HashMap<u32, (u64, u64)>;

#[derive(Default)]
pub struct ProcessIo {
    totals: Option<IoTotals>,
    deltas: Option<IoTotals>,
    has_warned: bool,
}

impl ProcessIo {
    pub fn refresh(&mut self) {
        let Some(totals) = self.read_helper() else {
            self.totals = None;
            self.deltas = None;
            return;
        };

        self.deltas = self.totals.as_ref().map(|previous| subtract(&totals, previous));
        self.totals = Some(totals);
    }

    pub fn usage(&self, process: &Process) -> (u64, u64) {
        match &self.deltas {
            Some(deltas) => {
                deltas.get(&process.pid().as_u32()).copied().unwrap_or_default()
            }
            None => {
                let usage = process.disk_usage();
                (usage.read_bytes, usage.written_bytes)
            }
        }
    }

    fn read_helper(&mut self) -> Option<IoTotals> {
        let stream = match UnixStream::connect(SOCKET_PATH) {
            Ok(stream) => stream,
            Err(error) => {
                if error.kind() != ErrorKind::NotFound && !self.has_warned {
                    warn!(
                        "could not reach the disk I/O helper at {SOCKET_PATH}: {error}"
                    );
                    self.has_warned = true;
                }
                return None;
            }
        };

        stream.set_read_timeout(Some(READ_TIMEOUT)).ok()?;
        let mut text = String::new();
        stream.take(MAX_BYTES).read_to_string(&mut text).ok()?;
        Some(parse_totals(&text))
    }
}

pub fn parse_totals(text: &str) -> IoTotals {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split(' ').map(str::parse::<u64>);
            let pid = u32::try_from(fields.next()?.ok()?).ok()?;
            let read = fields.next()?.ok()?;
            let write = fields.next()?.ok()?;
            fields.next().is_none().then_some((pid, (read, write)))
        })
        .collect()
}

fn subtract(totals: &IoTotals, previous: &IoTotals) -> IoTotals {
    totals
        .iter()
        .map(|(&pid, &(read, write))| {
            let (last_read, last_write) =
                previous.get(&pid).copied().unwrap_or((read, write));
            (pid, (read.saturating_sub(last_read), write.saturating_sub(last_write)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{parse_totals, subtract};

    #[test]
    fn parses_lines_and_skips_malformed_ones() {
        let totals = parse_totals("1 100 200\n42 5 6\nbad line\n7 1\n8 1 2 3\n-1 1 2\n");
        assert_eq!(totals.len(), 2);
        assert_eq!(totals[&1], (100, 200));
        assert_eq!(totals[&42], (5, 6));
    }

    #[test]
    fn subtracts_per_pid_and_starts_new_pids_at_zero() {
        let previous = parse_totals("1 100 200\n2 50 50\n");
        let totals = parse_totals("1 150 260\n2 10 10\n3 999 999\n");
        let deltas = subtract(&totals, &previous);
        assert_eq!(deltas[&1], (50, 60));
        assert_eq!(deltas[&2], (0, 0));
        assert_eq!(deltas[&3], (0, 0));
    }
}
