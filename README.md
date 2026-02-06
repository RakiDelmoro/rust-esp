# rust-esp
A project-based approach to learning embedded Rust on ESP32, from basic hardware control to real-world microcontroller applications.

Inside Docker Container Run:
```
cargo generate --git https://github.com/esp-rs/esp-idf-template.git --name example-project
```

Flash/Monitor Outside Docker Container Run:
```
espflash flash --chip esp32 --port COM5 path (.elf or .bin)
```
```
espflash monitor --port COM5
```
