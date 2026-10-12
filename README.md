# board

A real-time transit departure board built with [Slint](https://slint.dev/), showing Redwood City Caltrain arrivals and departures.

Live times come from Caltrain's GTFS-Realtime trip updates (`https://www.caltrain.com/files/rt/tripupdates/CT.json`). Destinations come from the static GTFS feed linked on [Caltrain's developer page](https://www.caltrain.com/developer-resources). Northbound platform `70141` and southbound platform `70142` are both included. No API key is required.

Designed to run on a PROXmobil3 with a framebuffer display, but also works as a desktop window for development.

## Usage

```
cargo run
```

## Building

### Desktop (x86, windowed)

```
cargo run
```

### ARM (PROXmobil3, framebuffer)

```
cargo zigbuild --release --target armv7-unknown-linux-gnueabihf.2.28 --no-default-features --features framebuffer
```

## Deploying to PROXmobil3

1. Build the ARM binary (see above).

2. Copy the binary to the device over SSH:
   ```
   cat target/armv7-unknown-linux-gnueabihf/release/board | ssh root@<DEVICE_IP> 'cat > /init/board/exe && chmod +x /init/board/exe'
   ```

3. Create the systemd service at `/init/board/board.service` on the device:
   ```ini
   [Unit]
   Description=Board Display Service
   After=network.target

   [Service]
   Type=simple
   ExecStart=/init/board/exe
   Restart=always
   RestartSec=2
   WorkingDirectory=/init/board

   [Install]
   WantedBy=multi-user.target
   ```

4. Create the autorun script at `/init/autorun/99-board.sh` to install the service on boot:
   ```sh
   #!/bin/sh

   mount -o remount,rw /

   # Set timezone (adjust as needed)
   ln -sf /usr/share/zoneinfo/America/Los_Angeles /etc/localtime

   # Disable default PM3 UI
   systemctl stop nx
   systemctl mask nx
   systemctl stop init-abtproxy
   systemctl mask init-abtproxy

   # Install and start board service
   cp /init/board/board.service /etc/systemd/system/
   systemctl daemon-reload
   systemctl enable board.service

   mount -o remount,ro /

   /usr/bin/NxExe watchdog 0

   systemctl start board.service
   ```

5. To change the service later:
   ```
   ssh root@<DEVICE_IP>
   mount -o remount,rw /
   vi /init/board/board.service
   mount -o remount,ro /
   systemctl daemon-reload
   systemctl restart board.service
   ```

## Debug CLI

Inspect the live Redwood City feed and the rows that make it onto the board:

```
cargo run --bin debug_arrivals
```

## Development

Uses Nix for the development environment:

```
nix-shell
```

## License

[MIT](LICENSE)
