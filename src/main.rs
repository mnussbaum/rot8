extern crate clap;
extern crate glob;
use clap::{App, Arg};
use glob::glob;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[derive(Serialize, Deserialize)]
struct SwayOutput {
    name: String,
    transform: String,
}

fn get_window_manager_state(display: &str, mode: &str) -> Result<String, String> {
    if mode == "sway" {
        let raw_rotation_state = String::from_utf8(
            Command::new("swaymsg")
                .arg("-t")
                .arg("get_outputs")
                .arg("--raw")
                .output()
                .expect("Swaymsg get outputs command failed to start")
                .stdout,
        )
        .unwrap();
        let deserialized: Vec<SwayOutput> = serde_json::from_str(&raw_rotation_state).unwrap();
        for output in deserialized {
            if output.name == display {
                return Ok(output.transform);
            }
        }

        return Err("no match".to_owned());
    } else if mode == "x" {
        let raw_rotation_state = String::from_utf8(
            Command::new("xrandr")
                .output()
                .expect("Xrandr get outputs command failed to start")
                .stdout,
        )
        .unwrap();
        let xrandr_output_pattern = Regex::new(format!(
            r"^{} connected .+? .+? (normal |inverted |left |right )?\(normal left inverted right x axis y axis\) .+$",
            regex::escape(display),
        ).as_str()).unwrap();
        for xrandr_output_line in raw_rotation_state.split("\n") {
            println!("{:?}", xrandr_output_line);
            if !xrandr_output_pattern.is_match(xrandr_output_line) {
                continue;
            }

            // eDP-1 connected primary 3200x1800+0+0 (normal left inverted right x axis y axis) 294mm x 165mm
            let xrandr_output_captures =
                xrandr_output_pattern.captures(xrandr_output_line).unwrap();
            if let Some(transform) = xrandr_output_captures.get(1) {
                return Ok(transform.as_str().to_owned());
            } else {
                return Ok("normal".to_owned());
            }
        }

        return Err("no match".to_owned());
    } else {
        panic!()
    }
}

fn main() {
    let mut mode = "";
    let mut new_state: &str;
    let mut path_x: String = "".to_string();
    let mut path_y: String = "".to_string();
    let mut matrix: [&str; 9];
    let mut x_state: &str;

    let sway_pid =
        String::from_utf8(Command::new("pidof").arg("sway").output().unwrap().stdout).unwrap();

    let x_pid =
        String::from_utf8(Command::new("pidof").arg("Xorg").output().unwrap().stdout).unwrap();

    if sway_pid.len() >= 1 {
        mode = "sway";
    }
    if x_pid.len() >= 1 {
        mode = "x";
    }

    let matches = App::new("rot8")
        .version("0.1.1")
        .arg(
            Arg::with_name("sleep")
                .default_value("500")
                .long("sleep")
                .value_name("SLEEP")
                .help("Set sleep millis")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("display")
                .default_value("eDP-1")
                .long("display")
                .value_name("DISPLAY")
                .help("Set Display Device")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("touchscreen")
                .default_value("ELAN0732:00 04F3:22E1")
                .long("touchscreen")
                .value_name("TOUCHSCREEN")
                .help("Set Touchscreen Device (X11)")
                .takes_value(true),
        )
        .get_matches();
    let sleep = matches.value_of("sleep").unwrap_or("default.conf");
    let display = matches.value_of("display").unwrap_or("default.conf");
    let touchscreen = matches.value_of("touchscreen").unwrap_or("default.conf");
    let old_state_owned = get_window_manager_state(display, mode).unwrap();
    let mut old_state = old_state_owned.as_str();

    for entry in glob("/sys/bus/iio/devices/iio:device*/in_accel_*_raw").unwrap() {
        match entry {
            Ok(path) => {
                if path.to_str().unwrap().contains("x_raw") {
                    path_x = path.to_str().unwrap().to_owned();
                } else if path.to_str().unwrap().contains("y_raw") {
                    path_y = path.to_str().unwrap().to_owned();
                } else if path.to_str().unwrap().contains("z_raw") {
                    continue;
                } else {
                    println!("{:?}", path);
                    panic!();
                }
            }
            Err(e) => println!("{:?}", e),
        }
    }

    loop {
        let x_raw = fs::read_to_string(path_x.as_str()).unwrap();
        let y_raw = fs::read_to_string(path_y.as_str()).unwrap();
        let x = x_raw.trim_end_matches('\n').parse::<i32>().unwrap_or(0);
        let y = y_raw.trim_end_matches('\n').parse::<i32>().unwrap_or(0);

        if x < -500000 {
            if y > 500000 {
                new_state = "180";
                x_state = "normal";
                matrix = ["-1", "0", "1", "0", "-1", "1", "0", "0", "1"];
            } else {
                new_state = "90";
                x_state = "left";
                matrix = ["0", "-1", "1", "1", "0", "0", "0", "0", "1"];
            }
        } else if x > 500000 {
            if y > 500000 {
                new_state = "180";
                x_state = "inverted";
                matrix = ["-1", "0", "1", "0", "-1", "1", "0", "0", "1"];
            } else {
                new_state = "270";
                x_state = "right";
                matrix = ["0", "1", "0", "-1", "0", "1", "0", "0", "1"];
            }
        } else {
            if y > 500000 {
                new_state = "180";
                x_state = "inverted";
                matrix = ["-1", "0", "1", "0", "-1", "1", "0", "0", "1"];
            } else {
                new_state = "normal";
                x_state = "normal";
                matrix = ["1", "0", "0", "0", "1", "0", "0", "0", "1"];
            }
        }

        if new_state != old_state {
            if mode == "sway" {
                Command::new("swaymsg")
                    .arg("output")
                    .arg(display)
                    .arg("transform")
                    .arg(new_state)
                    .spawn()
                    .expect("rotate command failed to start");

                old_state = new_state;
            }
            if mode == "x" {
                Command::new("xrandr")
                    .arg("-o")
                    .arg(x_state)
                    .spawn()
                    .expect("rotate command failed to start");

                Command::new("xinput")
                    .arg("set-prop")
                    .arg(touchscreen)
                    .arg("Coordinate")
                    .arg("Transformation")
                    .arg("Matrix")
                    .args(&matrix)
                    .spawn()
                    .expect("rotate command failed to start");

                old_state = new_state;
            }
        }
        thread::sleep(Duration::from_millis(sleep.parse::<u64>().unwrap_or(0)));
    }
}
