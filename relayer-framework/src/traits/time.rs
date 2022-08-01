use core::time::Duration;

use crate::traits::core::Async;

pub trait Time: Async {
    fn duration_since(&self, other: &Self) -> Duration;
}

pub trait TimeContext: Async {
    type Time: Time;

    fn now(&self) -> Self::Time;
}
