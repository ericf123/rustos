// FIXME: Make me pass! Diff budget: 25 lines.

#[derive(Debug)]
enum Duration {
    MilliSeconds(u64),
    Seconds(u32),
    Minutes(u16),
}

// What traits does `Duration` need to implement?

impl PartialEq for Duration {
    fn eq(&self, other: &Self) -> bool {
        self.toMs() == other.toMs()
    }
}

impl Duration {
    fn toMs(&self) -> u64 {
        match *self {
            Duration::MilliSeconds(s) => s,
            Duration::Seconds(s) => (s * 1000) as u64,
            Duration::Minutes(s) => (s as u64) * 60 * 1000
        }
    }
}

#[test]
fn traits() {
    assert_eq!(Duration::Seconds(120), Duration::Minutes(2));
    assert_eq!(Duration::Seconds(420), Duration::Minutes(7));
    assert_eq!(Duration::MilliSeconds(420000), Duration::Minutes(7));
    assert_eq!(Duration::MilliSeconds(43000), Duration::Seconds(43));
}
