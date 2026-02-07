# rust-esp
A project-based approach to learning embedded Rust on ESP32, from basic hardware control to real-world microcontroller applications.

To start a project run the following inside docker container:
```
cargo generate --git https://github.com/esp-rs/esp-idf-template.git --name example-project
```

To Flash/Monitor run the following outside docker container:
```
espflash flash --chip esp32 --port COM5 path (.elf or .bin)
```
```
espflash monitor --port COM5
```
