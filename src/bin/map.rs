extern crate truckmap;
use std::thread::sleep;
use std::time::{Duration, Instant};
use truckmap::DataCollector;
const URL: &str = "https://tracker.ets2map.com/v2/fullmap";
// Get servers from https://truckersmp.krashnz.com/servers
fn main() {
    let mut now = Instant::now();
    let mut collector = DataCollector::new(URL, r"D:\TruckMap");
    collector.add_map("ATS", &[10, 11]);
    collector.add_map("ETS2", &[1, 3, 4, 5, 7, 8, 13]);
    if let Err(e) = collector.load() {
        println!("Error during loading: {:?}", e);
        return;
    }
    println!("Loaded data");
    if let Err(e) = collector.save() {
        println!("Error during saving: {:?}", e);
    } else {
        println!("Initial save complete");
    }
    loop {
        if let Err(e) = collector.update() {
            println!("Error during updating: {:?}", e);
        }
        if now.elapsed() > Duration::from_secs(60 * 60) {
            now = Instant::now();
            if let Err(e) = collector.save() {
                println!("Error during saving: {:?}", e);
            } else {
                let d = now.elapsed();
                println!("Saved in {} seconds", d.as_secs());
            }
        }
        sleep(Duration::from_secs(2));
    }
}
