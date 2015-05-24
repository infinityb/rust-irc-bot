
use std::collections::PriorityQueue;
use time::SteadyTime;


struct TimerEntry<T> {
    trigger_at: SteadyTime,
    data: T,
}

struct Timer(PriorityQueue<TimerEntry<T>>);

impl Timer {
    //
}
