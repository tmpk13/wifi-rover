use anyhow::Result;
use embedded_svc::{
    http::Method,
    io::Write,
    ws::{FrameType, Receiver},
};
use esp_idf_hal::{delay::FreeRtos, ledc::LedcDriver};
use esp_idf_svc::http::server::{ws::EspHttpWsConnection, EspHttpServer};
use std::sync::{Arc, Mutex};

use crate::config::{SERVO_CLOSE, SERVO_OPEN};
use crate::motor::Motors;
use crate::servo::set_angle;

const HTML_SITE: &str = r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Wifi Rover</title>
    <style>
        body { font-family: sans-serif; text-align: center; margin-top: 30px; background: #222; color: #fff; }
        h2 { color: #aaa; font-size: 1rem; text-transform: uppercase; letter-spacing: 2px; margin: 24px 0 8px; }
        button {
            display: inline-block; width: 80px; height: 80px; margin: 6px;
            font-size: 1.4rem; border: none; border-radius: 12px; cursor: pointer;
            background: #007BFF; color: white; transition: 0.15s;
        }
        button:active { background: #0056b3; transform: scale(0.95); }
        .wide { width: 174px; }
        .stop { background: #dc3545; }
        .stop:active { background: #a71d2a; }
        .servo { background: #28a745; }
        .servo:active { background: #1a6e30; }
        #status { color: #888; margin-top: 16px; }
    </style>
</head>
<body>
    <h1>Wifi Rover</h1>

    <h2>Drive</h2>
    <div>
        <button onclick="cmd('forward')">&#x25B2;</button>
    </div>
    <div>
        <button onclick="cmd('left')">&#x25C4;</button>
        <button class="stop" onclick="cmd('stop')">&#x25A0;</button>
        <button onclick="cmd('right')">&#x25BA;</button>
    </div>
    <div>
        <button onclick="cmd('backward')">&#x25BC;</button>
    </div>

    <h2>Servo</h2>
    <button class="servo wide" onclick="cmd('open')">Open</button>
    <button class="servo wide" onclick="cmd('close')">Close</button>
    <button class="servo wide" onclick="cmd('cycle')">Cycle</button>

    <p id="status">Connecting...</p>
    <script>
        let ws;
        function connect() {
            ws = new WebSocket('ws://' + location.host + '/ws');
            ws.onopen  = () => { document.getElementById('status').innerText = 'Ready'; };
            ws.onclose = () => {
                document.getElementById('status').innerText = 'Disconnected – reconnecting…';
                setTimeout(connect, 1500);
            };
        }
        connect();
        function cmd(c) {
            if (ws && ws.readyState === WebSocket.OPEN) ws.send(c);
            document.getElementById('status').innerText = c;
            setTimeout(() => document.getElementById('status').innerText = 'Ready', 800);
        }
    </script>
</body>
</html>
"#;

pub fn register_handlers(
    server: &mut EspHttpServer<'static>,
    servo: Arc<Mutex<LedcDriver<'static>>>,
    motors: Arc<Mutex<Motors<'static>>>,
) -> Result<()> {
    server.fn_handler("/", Method::Get, |request| -> Result<()> {
        let mut response = request.into_ok_response()?;
        response.write_all(HTML_SITE.as_bytes())?;
        Ok(())
    })?;

    server.ws_handler("/ws", None, move |ws: &mut EspHttpWsConnection| -> Result<()> {
        let mut buf = [0u8; 64];
        loop {
            match ws.recv(&mut buf) {
                Ok((FrameType::Text(_), len)) => {
                    match core::str::from_utf8(&buf[..len]).unwrap_or("") {
                        "forward"  => { motors.lock().unwrap().drive(100, 100)?; }
                        "backward" => { motors.lock().unwrap().stop()?; }
                        "left"     => { motors.lock().unwrap().drive(0, 100)?; }
                        "right"    => { motors.lock().unwrap().drive(100, 0)?; }
                        "stop"     => { motors.lock().unwrap().stop()?; }
                        "open"  => { set_angle(&mut servo.lock().unwrap(), SERVO_OPEN); }
                        "close" => { set_angle(&mut servo.lock().unwrap(), SERVO_CLOSE); }
                        "cycle" => {
                            let mut s = servo.lock().unwrap();
                            set_angle(&mut s, SERVO_OPEN);
                            drop(s);
                            FreeRtos::delay_ms(500);
                            let mut s = servo.lock().unwrap();
                            set_angle(&mut s, SERVO_CLOSE);
                            drop(s);
                            FreeRtos::delay_ms(500);
                            let mut s = servo.lock().unwrap();
                            set_angle(&mut s, SERVO_OPEN);
                        }
                        _ => {}
                    }
                }
                Ok((FrameType::Close, _)) | Err(_) => break,
                _ => {}
            }
        }
        Ok(())
    })?;

    Ok(())
}
