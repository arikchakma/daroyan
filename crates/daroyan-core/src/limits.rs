use std::collections::HashMap;
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use governor::{DefaultDirectRateLimiter, DefaultKeyedRateLimiter, Quota, RateLimiter};

use crate::error::{Error, ErrorName, Result};

const GLOBAL_PER_SECOND: NonZeroU32 = NonZeroU32::new(50).unwrap();
const GLOBAL_BURST: NonZeroU32 = NonZeroU32::new(100).unwrap();
const ENROLL_PER_MINUTE: NonZeroU32 = NonZeroU32::new(10).unwrap();
const CONNECT_PERIOD: Duration = Duration::from_secs(10);
const CONNECT_BURST: NonZeroU32 = NonZeroU32::new(10).unwrap();
const SOCKET_PER_SECOND: NonZeroU32 = NonZeroU32::new(20).unwrap();
const SOCKET_BURST: NonZeroU32 = NonZeroU32::new(200).unwrap();
const AUTH_FAILURE_LIMIT: u32 = 10;
const AUTH_FAILURE_WINDOW: Duration = Duration::from_secs(60);
const LOG_BUDGET_WINDOW: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct Limits {
    global: Arc<DefaultKeyedRateLimiter<IpAddr>>,
    enroll: Arc<DefaultKeyedRateLimiter<IpAddr>>,
    connect: Arc<DefaultKeyedRateLimiter<String>>,
    auth_failures: Arc<Mutex<HashMap<IpAddr, FailureWindow>>>,
}

struct FailureWindow {
    started_at: Instant,
    count: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            global: Arc::new(RateLimiter::keyed(
                Quota::per_second(GLOBAL_PER_SECOND).allow_burst(GLOBAL_BURST),
            )),
            enroll: Arc::new(RateLimiter::keyed(Quota::per_minute(ENROLL_PER_MINUTE))),
            connect: Arc::new(RateLimiter::keyed(
                Quota::with_period(CONNECT_PERIOD)
                    .unwrap_or_else(|| Quota::per_second(NonZeroU32::MIN))
                    .allow_burst(CONNECT_BURST),
            )),
            auth_failures: Arc::default(),
        }
    }
}

fn to_key(ip: IpAddr) -> IpAddr {
    match ip.to_canonical() {
        IpAddr::V6(ip) => IpAddr::V6((u128::from(ip) & !u128::from(u64::MAX)).into()),
        ip @ IpAddr::V4(_) => ip,
    }
}

impl Limits {
    pub fn check_global(&self, ip: IpAddr) -> Result<()> {
        self.global.check_key(&to_key(ip)).map_err(|_| too_many_requests())
    }

    pub fn check_enroll(&self, ip: IpAddr) -> Result<()> {
        self.enroll.check_key(&to_key(ip)).map_err(|_| too_many_requests())
    }

    pub fn check_connect(&self, server_id: &str) -> Result<()> {
        self.connect
            .check_key(&server_id.to_string())
            .map_err(|_| too_many_requests())
    }

    pub fn check_auth_blocked(&self, ip: IpAddr) -> Result<()> {
        let failures = self.lock_failures();
        let is_blocked = failures.get(&to_key(ip)).is_some_and(|window| {
            window.count >= AUTH_FAILURE_LIMIT
                && window.started_at.elapsed() < AUTH_FAILURE_WINDOW
        });
        if is_blocked { Err(too_many_requests()) } else { Ok(()) }
    }

    pub fn record_auth_failure(&self, ip: IpAddr) {
        let mut failures = self.lock_failures();
        let window = failures
            .entry(to_key(ip))
            .or_insert(FailureWindow { started_at: Instant::now(), count: 0 });
        if window.started_at.elapsed() >= AUTH_FAILURE_WINDOW {
            *window = FailureWindow { started_at: Instant::now(), count: 0 };
        }
        window.count += 1;
    }

    pub fn forget_stale(&self) {
        self.global.retain_recent();
        self.enroll.retain_recent();
        self.connect.retain_recent();
        self.global.shrink_to_fit();
        self.enroll.shrink_to_fit();
        self.connect.shrink_to_fit();
        self.lock_failures()
            .retain(|_, window| window.started_at.elapsed() < AUTH_FAILURE_WINDOW);
    }

    fn lock_failures(&self) -> MutexGuard<'_, HashMap<IpAddr, FailureWindow>> {
        self.auth_failures.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub fn new_socket_limiter() -> DefaultDirectRateLimiter {
    RateLimiter::direct(Quota::per_second(SOCKET_PER_SECOND).allow_burst(SOCKET_BURST))
}

#[derive(Clone)]
pub struct LogBudgets {
    limit: u64,
    windows: Arc<Mutex<HashMap<String, BudgetWindow>>>,
}

struct BudgetWindow {
    started_at: Instant,
    used: u64,
}

impl LogBudgets {
    pub fn new(limit: u64) -> Self {
        Self { limit, windows: Arc::default() }
    }

    pub fn spend(&self, key: &str, size: u64) -> Result<(), Duration> {
        let mut windows = self.windows.lock().unwrap_or_else(PoisonError::into_inner);
        let window = windows
            .entry(key.to_string())
            .or_insert(BudgetWindow { started_at: Instant::now(), used: 0 });
        let elapsed = window.started_at.elapsed();
        if elapsed >= LOG_BUDGET_WINDOW {
            *window = BudgetWindow { started_at: Instant::now(), used: 0 };
        }

        // A batch bigger than the whole budget still goes through at the start
        // of a window, or its client would retry it forever.
        if window.used > 0 && window.used + size > self.limit {
            return Err(LOG_BUDGET_WINDOW.saturating_sub(window.started_at.elapsed()));
        }

        window.used += size;
        Ok(())
    }
}

fn too_many_requests() -> Error {
    Error::new(ErrorName::RateLimitExceeded, "Too many requests. Please try again later.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_an_ip_after_ten_auth_failures() {
        let limits = Limits::default();
        let ip: IpAddr = "203.0.113.7".parse().unwrap();
        let other: IpAddr = "203.0.113.8".parse().unwrap();
        for _ in 0..AUTH_FAILURE_LIMIT {
            assert!(limits.check_auth_blocked(ip).is_ok());
            limits.record_auth_failure(ip);
        }
        assert!(limits.check_auth_blocked(ip).is_err());
        assert!(limits.check_auth_blocked(other).is_ok());
    }

    #[test]
    fn limits_reconnects_per_server() {
        let limits = Limits::default();
        for _ in 0..CONNECT_BURST.get() {
            assert!(limits.check_connect("srv_a").is_ok());
        }
        assert!(limits.check_connect("srv_a").is_err());
        assert!(limits.check_connect("srv_b").is_ok());
    }

    #[test]
    fn lets_an_agent_flush_its_queue_then_limits_it() {
        let limiter = new_socket_limiter();
        assert!((0..61).all(|_| limiter.check().is_ok()));
        let mut allowed = 61;
        while limiter.check().is_ok() {
            allowed += 1;
        }
        assert_eq!(allowed, SOCKET_BURST.get());
    }

    #[test]
    fn refuses_a_batch_over_the_budget_until_the_window_resets() {
        let budgets = LogBudgets::new(10);
        assert!(budgets.spend("ing_1", 8).is_ok());

        let retry_after = budgets.spend("ing_1", 4).unwrap_err();
        assert!(
            retry_after <= LOG_BUDGET_WINDOW && retry_after > Duration::from_secs(55)
        );
        assert!(budgets.spend("ing_1", 2).is_ok());

        assert!(budgets.spend("ing_2", 50).is_ok());
        assert!(budgets.spend("ing_2", 1).is_err());
    }

    #[test]
    fn groups_ipv6_by_prefix() {
        let limits = Limits::default();
        for host in 1..=AUTH_FAILURE_LIMIT {
            limits.record_auth_failure(format!("2001:db8:1:2::{host}").parse().unwrap());
        }
        assert!(
            limits
                .check_auth_blocked("2001:db8:1:2:ffff::9".parse().unwrap())
                .is_err()
        );
        assert!(limits.check_auth_blocked("2001:db8:1:3::1".parse().unwrap()).is_ok());
        assert!(limits.check_auth_blocked("203.0.113.8".parse().unwrap()).is_ok());
    }
}
