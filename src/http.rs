use anyhow::Result;
use embedded_svc::{
    http::Method,
    io::Write,
    ws::{FrameType, Receiver},
};
use esp_idf_hal::{delay::FreeRtos, ledc::LedcDriver};
use esp_idf_svc::http::server::{ws::EspHttpWsConnection, EspHttpServer};
use std::sync::{Arc, Mutex};

use crate::config::{SERVO_CLOSE, SERVO_OPEN, SERVO_CENTER, SERVO_LEFT, SERVO_RIGHT};
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
        <button onpointerdown="send('forward')" onpointerup="send('stop')" onpointerleave="send('stop')" onpointercancel="send('stop')">&#x25B2;</button>
    </div>
    <div>
        <button onpointerdown="send('left')" onpointerup="send('center')" onpointerleave="send('center')" onpointercancel="send('center')">&#x25C4;</button>
        <button class="stop" onpointerdown="send('stop');send('center')">&#x25A0;</button>
        <button onpointerdown="send('right')" onpointerup="send('center')" onpointerleave="send('center')" onpointercancel="send('center')">&#x25BA;</button>
    </div>
    <div>
        <button onpointerdown="send('backward')" onpointerup="send('stop')" onpointerleave="send('stop')" onpointercancel="send('stop')">&#x25BC;</button>
    </div>

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
        function send(c) { if (ws && ws.readyState === WebSocket.OPEN) ws.send(c); }
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
        if matches!(ws, EspHttpWsConnection::New(..)) {
            log::info!("WS: client connected");
            return Ok(());
        }
        if matches!(ws, EspHttpWsConnection::Closed(..)) {
            log::info!("WS: client disconnected");
            return Ok(());
        }
        let mut buf = [0u8; 64];
        match ws.recv(&mut buf) {
            Ok((FrameType::Text(_), len)) => {
                let cmd = core::str::from_utf8(&buf[..len]).unwrap_or("?").trim_end_matches('\0');
                log::info!("WS cmd: {}", cmd);
                match cmd {
                    "forward"  => { motors.lock().unwrap().drive(100, 0)?; }
                    "backward" => { motors.lock().unwrap().drive(0, 100)?; }
                    "stop"     => { motors.lock().unwrap().stop()?; }
                    "left"     => { set_angle(&mut servo.lock().unwrap(), SERVO_LEFT); }
                    "right"    => { set_angle(&mut servo.lock().unwrap(), SERVO_RIGHT); }
                    "center"   => { set_angle(&mut servo.lock().unwrap(), SERVO_CENTER); }
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
                    other => { log::warn!("WS: unknown cmd '{}'", other); }
                }
            }
            Ok((frame_type, _)) => { log::debug!("WS: unhandled frame {:?}", frame_type); }
            Err(e) => { log::warn!("WS recv error: {:?}", e); }
        }
        Ok(())
    })?;

    Ok(())
}
