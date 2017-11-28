#![feature(try_trait)]
extern crate image;
extern crate memmap;
extern crate reqwest;
extern crate serde;
#[macro_use] extern crate serde_derive;
extern crate serde_json;
extern crate winapi;

mod color;

use image::{ColorType, ImageLuma8, open, save_buffer};
use memmap::MmapMut;
use serde_json::{Value, from_slice, from_value};
use std::collections::{HashMap, HashSet};
use std::fs::{OpenOptions, create_dir, read_dir, rename};
use std::io::Read;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use winapi::FILE_SHARE_READ;

#[derive(Debug)]
pub enum Error {
    Image(image::ImageError),
    Io(std::io::Error),
    ParseInt(std::num::ParseIntError),
    Reqwest(reqwest::Error),
    SerdeJson(serde_json::Error),
    None,
}
impl From<image::ImageError> for Error {
    fn from(e: image::ImageError) -> Error {
        Error::Image(e)
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::Io(e)
    }
}
impl From<std::num::ParseIntError> for Error {
    fn from(e: std::num::ParseIntError) -> Error {
        Error::ParseInt(e)
    }
}
impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Error {
        Error::Reqwest(e)
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Error {
        Error::SerdeJson(e)
    }
}
impl From<std::option::NoneError> for Error {
    fn from(_: std::option::NoneError) -> Error {
        Error::None
    }
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Deserialize)]
struct Truck {
    name: String,
    h: f64,
    p_id: String,
    server: i64,
    mp_id: i64,
    t: i64,
    online: bool,
    y: i64,
    x: i64,
}

struct Tile {
    map: MmapMut,
}
impl Tile {
    fn set(&mut self, x: u32, y: u32) {
        let index = y * 1024 + x;
        let byte_index = index >> 3;
        let bit_index = index & 7;
        self.map[byte_index as usize] |= 1 << bit_index;
    }
    fn to_image(&self) -> Image {
        let data: Vec<f32> = self.map.iter().flat_map(|byte| {
            (0..8).map(move|i| if byte & (1 << i) == 0 { 0. } else { 1. })
        }).collect();
        Image {
            data: data,
        }
    }
    fn save(&self, path: &Path) -> Result<()> {
        let temp = path.with_file_name("temp.png");
        let img = self.to_image();
        img.save(&temp)?;
        rename(&temp, path)?;
        Ok(())
    }
    fn load(path: &Path) -> Result<Tile> {
        let file = OpenOptions::new().read(true).write(true).create(true)
            .share_mode(FILE_SHARE_READ).open(path)?;
        file.set_len(1024 * 128)?;
        let map = unsafe { MmapMut::map_mut(&file)? };
        Ok(Tile {
            map: map,
        })
    }
}

struct Image {
    data: Vec<f32>,
}
impl Image {
    fn new() -> Image {
        Image {
            data: vec![0.; 1024 * 1024],
        }
    }
    fn load_scaled(path: &Path, x: i64, y: i64) -> Result<Image> {
        let path = path.join(x.to_string()).join(format!("{}.png", y));
        use color::decode_srgb as dec;
        let ref img = match open(&path)? {
            ImageLuma8(img) => img,
            foo => foo.to_luma(),
        }.into_raw();
        assert!(img.len() == 1024 * 1024);
        let data = (0..512).flat_map(|y| {
            let yi = y * 2048;
            (0..512).map(move|x| {
                let i = yi + x * 2;
                let x = dec(img[i]) + dec(img[i + 1]) + dec(img[i + 1024]) + dec(img[i + 1025]);
                x * 0.25
            })
        }).collect();
        Ok(Image {
            data: data,
        })
    }
    fn save(&self, path: &Path) -> Result<()> {
        let pixels: Vec<u8> = self.data.iter().map(|&p| color::encode_srgb(p)).collect();
        save_buffer(path, &pixels, 1024, 1024, ColorType::Gray(8))?;
        Ok(())
    }
    fn copy_at_offset(&mut self, o: &Image, offset: usize) {
        for y in 0..512 {
            let from = y * 512;
            let to = offset + y * 1024;
            self.data[to..to + 512].copy_from_slice(&o.data[from..from + 512]);
        }
    }
    fn load_and_scale(path: &Path, x: i64, y: i64) -> Result<Image> {
        let mut image = Image::new();
        if let Ok(img) = Image::load_scaled(path, x << 1, y << 1) {
            image.copy_at_offset(&img, 0);
        }
        if let Ok(img) = Image::load_scaled(path, (x << 1) + 1, y << 1) {
            image.copy_at_offset(&img, 512);
        }
        if let Ok(img) = Image::load_scaled(path, x << 1, (y << 1) + 1) {
            image.copy_at_offset(&img, 1024 * 512);
        }
        if let Ok(img) = Image::load_scaled(path, (x << 1) + 1, (y << 1) + 1) {
            image.copy_at_offset(&img, 1024 * 512 + 512);
        }
        Ok(image)
    }

}

struct Map {
    name: String,
    tiles: HashMap<(i64, i64), Tile>,
    path: PathBuf,
}
impl Map {
    fn new(name: String, path: &Path) -> Map {
        let path = path.join(&name);
        Map {
            name: name,
            tiles: HashMap::new(),
            path: path,
        }
    }
    fn set(&mut self, x: i64, y: i64) {
        let (tx, ty) = (x >> 10, y >> 10);
        let (ix, iy) = (x & 1023, y & 1023);
        let path = &self.path;
        let tile = self.tiles.entry((tx, ty)).or_insert_with(|| {
            let px = path.join("raw").join(x.to_string());
            let _ = create_dir(&px);
            let pxy = px.join(format!("{}.dat", y));
            Tile::load(&pxy).unwrap()
        });
        tile.set(ix as u32, iy as u32);
    }
    fn save_tiles(&self, path: &Path) -> Result<()> {
        let path_base = path.join("0");
        let _ = create_dir(&path_base);
        for (&(x, y), tile) in &self.tiles {
            let px = path_base.join(x.to_string());
            let _ = create_dir(&px);
            let pxy = px.join(format!("{}.png", y));
            tile.save(&pxy)?;
        }
        Ok(())
    }
    fn save_level(&self, level: u8) -> Result<()> {
        let path_base = self.path.join((level - 1).to_string());
        let path_scaled = self.path.join(level.to_string());
        let _ = create_dir(&path_scaled);
        let todo: HashSet<(i64, i64)> = self.tiles.keys()
            .map(|&(x, y)| (x >> level, y >> level)).collect();
        for (x, y) in todo {
            let img = Image::load_and_scale(&path_base, x, y)?;
            let px = path_scaled.join(x.to_string());
            let _ = create_dir(&px);
            img.save(&px.join(format!("{}.png", y)))?;
        }
        Ok(())
    }
    fn save(&self, path: &Path) -> Result<()> {
        let path = path.join(&self.name);
        let _ = create_dir(&path);
        self.save_tiles(&path)?;
        for level in 1..8 {
            self.save_level(level)?;
        }
        Ok(())
    }
    fn load(&mut self, path: &Path) -> Result<()> {
        let path = path.join(&self.name);
        let path_tiles = path.join("raw");
        for entry in read_dir(path_tiles)? {
            let entry = entry?;
            let x: i64 = entry.file_name().to_str()?.parse()?;
            for entry in read_dir(entry.path())? {
                let entry = entry?;
                let tile_path = entry.path();
                let y: i64 = tile_path.file_stem()?.to_str()?.parse()?;
                let tile = Tile::load(&tile_path)?;
                self.tiles.insert((x, y), tile);
            }
        }
        Ok(())
    }
}

pub struct DataCollector {
    maps: Vec<Map>,
    url: String,
    servers: HashMap<i64, usize>,
    path: PathBuf,
}
impl DataCollector {
    pub fn new<T: Into<String>, P: Into<PathBuf>>(url: T, path: P) -> DataCollector {
        DataCollector {
            maps: Vec::new(),
            url: url.into(),
            servers: HashMap::new(),
            path: path.into(),
        }
    }
    pub fn add_map<T: Into<String>>(&mut self, name: T, servers: &[i64]) {
        let index = self.maps.len();
        self.maps.push(Map::new(name.into(), &self.path));
        for &server in servers {
            self.servers.insert(server, index);
        }
    }
    pub fn load(&mut self) -> Result<()> {
        for map in &mut self.maps {
            map.load(&self.path)?;
        }
        Ok(())
    }
    pub fn save(&self) -> Result<()> {
        let _ = create_dir(&self.path);
        for map in &self.maps {
            map.save(&self.path)?;
        }
        Ok(())
    }
    pub fn update(&mut self) -> Result<()> {
        let mut resp = reqwest::get(&self.url)?;
        if !resp.status().is_success() {
            println!("Status: {}", resp.status());
        }
        let mut data = Vec::new();
        resp.read_to_end(&mut data)?;
        let json: Value = from_slice(&data)?;
        let mut object = match json {
            Value::Object(x) => x,
            _ => return Err(Error::None),
        };
        let trucks = match object.remove("Trucks")? {
            Value::Object(x) => x,
            _ => return Err(Error::None),
        };
        for (_, truck) in trucks {
            let truck: Truck = from_value(truck)?;
            let server = match self.servers.get(&truck.server) {
                Some(&server) => server,
                None => continue,
            };
            if truck.x.abs() > 1_000_000 || truck.y.abs() > 1_000_000 { continue }
            self.maps[server].set(truck.x, truck.y);
        }
        Ok(())
    }
}
