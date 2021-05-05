// Copyright (C) 2021 Aayla Semyonova
// 
// This file is part of pipboy.
// 
// pipboy is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
// 
// pipboy is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
// 
// You should have received a copy of the GNU General Public License
// along with pipboy.  If not, see <http://www.gnu.org/licenses/>.

use std::path::Path;
use std::fs;
use std::fs::File;
use std::io::Write;
use substring::Substring;
use std::collections::HashMap;

use super::archives;
use super::profile;
use super::config_file;
use super::remote;

pub(crate) fn install_mod(config_path: &str, mod_value: &str, verbose: &bool, force: &bool) -> Result<(), String> {
    // Get current profile
    let profile_name = match config_file::current_profile(&config_path) {
        Ok(value) => value,
        Err(issue) => return Err(format!("Failed to get current profile <- {}", issue))
    };
    // Load profile file
    let profile = match profile::load_profile_file(&format!("{}/profiles/{}/profile", &config_path, &profile_name)) {
        Ok(value) => value,
        Err(_) => return Err("Failed to load profile file".to_string())
    };
    // Test if mod is already installed
    match mod_is_installed(&config_path, &mod_value) {
        Ok(result) => match result {
            true => return Err("Mod is already installed!".to_string()),
            false => {}
        }
        Err(issue) => return Err(format!("Failed to test if mod is already installed <- {}", issue))
    };
    // Check if the mod is cached locally
    match mod_is_cached(&config_path, &mod_value) {
        Ok(value) => match value {
            // Use local cache if present
            true => { println!("Using locally cached version of {}", &mod_value) },
            // Download mod if not present
            false => match remote::fetch_mod(&config_path, &mod_value) {
                Ok(_) => { println!("Downloaded {} from remote server", &mod_value) }
                Err(issue) => return Err(format!("Failed to fetch mod from remote server <- {}", issue))
            },
        },
        Err(issue) => return Err(format!("Failed to search mod cache <- {}", issue))
    };
    // Create an index file if the mod does not have one
    if !mod_has_index(&config_path, &mod_value) {
        generate_index(&config_path, &mod_value, &verbose);
    }
    // Check for file conflits
    match *force {
        false => match test_file_conflicts(&config_path, &mod_value, &verbose) {
            Ok(value) => match value {
                // File conflict detected
                true => return Err("File conflict detected!".to_string()),
                false => { }
            },
            Err(issue) => return Err(format!("Failed to test for file conflicts <- {}", issue))
        },
        true => println!("Force flag given. Skipping testing for file conflicts.")
    };
    // Install the mod
    let tarball_path = format!("{}/mods/cached/{}/mod.tar.gz", &config_path, &mod_value);
    return match archives::unpack_tarball(&tarball_path, &profile.install_path) {
        Ok(_) => Ok(()),
        Err(_) => Err("Failed to extract tarball!".to_string())
    };
}

pub(crate) fn generate_index(config_path: &str, mod_value: &str, verbose: &bool) -> Result<(), &'static str> {
    let mod_values = split_mod_value(mod_value);
    let mod_author = &mod_values[0];
    let mod_name = &mod_values[1];
    println!("{}", format!("Generating file index for {}/{}", &mod_author, &mod_name));
    // Create mod path
    let mod_path = format!("{}/mods/cached/{}/{}/mod.tar.gz", &config_path, &mod_author, &mod_name);
    let index_path = format!("{}/mods/indices/{}/{}/index", &config_path, &mod_author, &mod_name);
    // Create indices path if it doesn't exist
    if !Path::new(&index_path).parent().unwrap().exists() {
        fs::create_dir_all(Path::new(&index_path).parent().unwrap()).expect("Unable to create indices folder. Ensure you have permission to do this.");
    }
    if Path::new(&mod_path).exists() {
        let mod_contents = archives::list_contents(&mod_path);
        let mut f = File::create(&index_path).expect("Cannot create index file! Ensure you have permission to do this.");
        for item in mod_contents {
            if &item.chars().last().unwrap() != &'/' {
                f.write(format!("{}\n", item).as_bytes()).expect("Failed to write index file!");
                if *verbose {
                    println!("{}", &item);
                }
            }
        }
        Ok(())
    } else {
        Err("Can't index a mod that doesn't exist!")
    }
}

pub(crate) fn mod_has_index(config_path: &str, mod_value: &str) -> bool {
    let mod_values = split_mod_value(mod_value);
    let mod_author = &mod_values[0];
    let mod_name = &mod_values[1];
    let index_path = format!("{}/mods/indices/{}/{}/index", &config_path, &mod_author, &mod_name);
    return Path::new(&index_path).exists();
}

pub(crate) fn split_mod_value(mod_value: &str) -> Vec<String> {
    // I should be collecting this iterator but I don't know how
    let mut vec = Vec::new();
    for part in mod_value.split("/") {
        vec.push(part.to_string());
    }
    return vec;
}

pub(crate) fn mod_is_cached(config_path: &str, mod_value: &str) -> Result<bool, String> {
    // Define mod cache path
    let mod_cache_path: &str = &format!("{}/mods/cached/", &config_path);
    // Ensure the mod cache exists before trying to search it
    if !Path::new(&mod_cache_path).exists() {
        match fs::create_dir_all(&mod_cache_path) {
            Ok(_) => { },
            Err(_) => return Err("Failed to create mod cache path!".to_string())
        };
    }
    // Search the mod cache for a mod
    let mod_path: &str = &format!("{}/{}/mod.tar.gz", &mod_cache_path, &mod_value);
    // Return the value
    if Path::new(&mod_path).exists() {
        Ok(true)
    } else {
        Ok(false)
    }
}

pub(crate) fn test_file_conflicts(config_path: &str, mod_value: &str, verbose: &bool) -> Result<bool, &'static str> {
    // Get mod index path
    let index_path = format!("{}/mods/indices/{}/index", &config_path, &mod_value);
    // Get Data path
    let data_path = match config_file::load_config_file(&config_path) {
        Ok(config) => match profile::load_profile_file(format!("{}/profiles/{}/profile", &config_path, &config.current_profile)) {
            Ok(profile) => profile.install_path,
            Err(_) => return Err("Failed to load profile file!")
        },
        Err(_) => return Err("Failed to load configuration file!")
    };
    // Create index path if it doesn't exist
    if !Path::new(&index_path).exists() {
        match generate_index(&config_path, &mod_value, &verbose) {
            Ok(_) => { println!("Generated index for {}", &mod_value) },
            Err(issue) => return Err(format!("Failed to generate mod index <- {}", issue))
        };
    }
    // Ensure the data path exists
    if !Path::new(&data_path).exists() {
        return Err("Installation path does not exist!");
    }
    // Load mod index file
    let files: String = fs::read_to_string(&index_path)
        .unwrap().parse().unwrap();
    // Iterate over mod files and see if they would conflict with another file
    for item in files.lines() {
        // Only test files that are going into the Data/ path
        if item.substring(0, 5) == "Data/" && item != "Data/" {
            let outpath = format!("{}/{}", &data_path, &item);
            if Path::new(&outpath).exists() {
                println!("File conflict: {}", &item);
                return Ok(true);
            } else {
                if *verbose {
                    println!("No conflict: {}", &item);
                }
            }
        }
    }
    Ok(false)
}

pub(crate) fn log_files(config_path: &str, mod_value: &str, action: &str, verbose: bool) -> Result<(), &'static str> {
    // Get current profile
    let current_profile = match config_file::current_profile(&config_path) {
        Ok(profile) => profile,
        Err(issue) => return Err(format!("Failed to get current profile <- {}", issue))
    };
    let index_path = format!("{}/mods/indices/{}/index", &config_path, &mod_value);
    let dict_path = format!("{}/profiles/{}/file_associations.json", &config_path, &current_profile);
    // Test that files exist and create them if they don't
    let mut dictionary = HashMap::new();
    if !Path::new(&dict_path).exists() {
        if verbose {
            println!("File association dictionary does not exist. Creating new one.");
        }
    } else {
        let j: String = fs::read_to_string(&dict_path).unwrap().parse().unwrap();
        let dict_load: HashMap<String, String> = serde_json::from_str(&j).unwrap();
        for item in dict_load {
            dictionary.insert(item.0, item.1);
        }
    }
    // Test that mod index exists
    if !Path::new(&index_path).exists() {
        // Generate index if it doesn't exist
        println!("Index for {} does not exist. Generating.", &mod_value);
        generate_index(&config_path, &mod_value, &verbose);
    }
    // Load mod index
    let mod_index: String = fs::read_to_string(&index_path)
        .unwrap().parse().unwrap();
    // Preform the appropriate action
    match action {
        "install" => {
            for file in mod_index.lines() {
                let file_string = file.to_string();
                // Remove the value if the file was already in use by another mod
                if dictionary.contains_key(&file_string) {
                    
                    dictionary.remove(&file_string);
                }
                // Define the file's owner
                dictionary.insert(
                    file_string,
                    mod_value.to_string(),
                );
            }
        }
        "uninstall" => {
            for file in mod_index.lines() {
                let file_string = file.to_string();
                // Remove the value if it exists
                if dictionary.contains_key(&file_string) && dictionary.get(&file_string).unwrap() == &mod_value {
                    dictionary.remove(&file_string);
                }
            }
        }
        _ => {
            println!("log_files() was called with an invalid action!");
            return Err("Invalid action");
        }
    }
    // Serialize the dictionary
    let j = serde_json::to_string(&dictionary).unwrap();
    fs::write(&dict_path, &j).expect("Failed to serialize file association dictionary.");
    return Ok(());
}

pub(crate) fn uninstall_mod(config_path: &str, current_profile: &str, data_path: &str, mod_value: &str, verbose: bool) {
    // Load file ownership dictionary
    let dict_path = format!("{}/profiles/{}/file_associations.json", &config_path, &current_profile);
    let j: String = fs::read_to_string(&dict_path).unwrap().parse().unwrap();
    let dictionary: HashMap<String, String> = serde_json::from_str(&j).unwrap();
    for item in dictionary {
        if &item.1 == &mod_value {
            let remove_path = format!("{}/{}", &data_path, &item.0);
            if verbose {
                println!("Removing: {}", &remove_path);
            }
            fs::remove_file(&remove_path).expect(&format!("Failed to remove file {}", &remove_path));
        }
    }
}

pub(crate) fn mod_is_installed(config_path: &str, mod_value: &str) -> Result<bool, String> {
    // Get current profile
    return match config_file::load_config_file(&config_path) {
        Ok(config) => Ok(config.repository_list.contains(&mod_value.to_string())),
        Err(_) => Err("Failed to load configuration file".to_string())
    };
}