use std::io::{self, Read};
use std::time::{Instant, Duration};


pub struct SlowReadProtect<R> {
    reader: R,
    started_at: Instant,
    grace_time: Duration,
    bytes_rx: u64,
    min_bps: u32,
}

impl<R> SlowReadProtect<R>
    where R: Read
{
    pub fn new(reader: R, min_bps: u32) -> SlowReadProtect<R> {
        SlowReadProtect {
            reader: reader,
            started_at: Instant::now(),
            grace_time: Duration::new(5, 0),
            bytes_rx: 0,
            min_bps: min_bps,
        }
    }
}

impl<R> Read for SlowReadProtect<R> where R: Read {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let rx = self.reader.read(buf)?;
        self.bytes_rx += rx as u64;
        check_limits(self)?;
        Ok(rx)
    }
}

fn check_limits<R: Read>(ss: &SlowReadProtect<R>) -> io::Result<()> {
    let elapsed = ss.started_at.elapsed();

    // ignore the grace time
    if elapsed < ss.grace_time {
        return Ok(());
    }

    if ss.bytes_rx <= min_bytes(&elapsed, ss.min_bps) {
        Err(io::Error::new(io::ErrorKind::Other, "Read operations proceeding too slowly"))
    } else {
        Ok(())
    }
}

fn min_bytes(dur: &Duration, min_bps: u32) -> u64 {
    // a fun and useless micro-optimisation might be
    // using purely integer math here.
    let mut seconds = dur.as_secs() as f64;
    seconds += dur.subsec_nanos() as f64 / 1.0e+9;

    (seconds * min_bps as f64) as u64
}
