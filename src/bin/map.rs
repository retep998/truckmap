extern crate truckmap;
use std::thread::sleep;
use std::time::{Duration, Instant};
use truckmap::DataCollector;
const URL: &str = "https://tracker.ets2map.com/v2/fullmap";
fn main() {
    let mut now = Instant::now();
    let mut collector = DataCollector::new(URL, r"D:\TruckMap");
    collector.add_map("ATS", &[10, 11]);
    collector.add_map("ETS2", &[1, 3, 4, 5, 7, 8, 13]);
    if let Err(e) = collector.load() {
        println!("{:?}", e);
        return;
    }
    if let Err(e) = collector.save() {
        println!("{:?}", e);
    }
    println!("Initial save complete");
    loop {
        if let Err(e) = collector.update() {
            println!("{:?}", e);
        }
        if now.elapsed() > Duration::from_secs(60 * 60) {
            now = Instant::now();
            if let Err(e) = collector.save() {
                println!("{:?}", e);
            }
            let d = now.elapsed();
            println!("Saved in {} seconds", d.as_secs());
        }
        sleep(Duration::from_secs(2));
    }
}
