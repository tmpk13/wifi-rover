use anyhow::Result;
use embedded_svc::{
    http::Method,
    io::Write,
    ws::{FrameType, Receiver},
};
use esp_idf_hal::ledc::LedcDriver;
use esp_idf_svc::http::server::{ws::EspHttpWsConnection, EspHttpServer};
use std::sync::{Arc, Mutex};

use crate::motor::Motors;
use crate::servo::set_angle;

const HTML_SITE: &str = r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1, user-scalable=no">
    <title>Wifi Rover</title>
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body {
            font-family: sans-serif; text-align: center;
            background: #1a1a1a; color: #fff;
            height: 100dvh;
            display: flex; flex-direction: column;
            align-items: center; justify-content: center;
            touch-action: none; user-select: none; -webkit-user-select: none;
        }
        h1 { font-size: 1rem; color: #555; letter-spacing: 4px; text-transform: uppercase; margin-bottom: 32px; }
        #base {
            position: relative;
            width: 260px; height: 260px;
            border-radius: 12px;
            background: #242424;
            border: 2px solid #383838;
            cursor: crosshair;
            overflow: hidden;
            touch-action: none;
        }
        #base::before, #base::after {
            content: '';
            position: absolute;
            background: rgba(255,255,255,0.07);
            pointer-events: none;
        }
        #base::before { width: 1px; height: 100%; left: 50%; transform: translateX(-50%); }
        #base::after  { height: 1px; width: 100%; top: 50%;  transform: translateY(-50%); }
        #dz-x, #dz-y {
            position: absolute;
            background: rgba(255,255,255,0.045);
            border: 1px solid rgba(255,255,255,0.07);
            pointer-events: none;
        }
        #thumb {
            position: absolute;
            width: 72px; height: 72px;
            border-radius: 50%;
            background: #1a6bcc;
            border: 2px solid rgba(255,255,255,0.18);
            transform: translate(-50%, -50%);
            left: 50%; top: 50%;
            pointer-events: none;
            box-shadow: 0 2px 14px rgba(0,0,0,0.6), 0 0 22px rgba(26,107,204,0.28);
            transition: background 0.08s;
        }
        #status {
            margin-top: 20px; color: #484848;
            font-size: 0.78rem; font-family: monospace;
            min-height: 1.2em; letter-spacing: 1px;
        }
        #settings {
            width: 260px; margin-top: 16px;
        }
        #settings summary {
            font-size: 0.72rem; color: #484848; letter-spacing: 2px;
            text-transform: uppercase; cursor: pointer; user-select: none;
            list-style: none; text-align: center;
        }
        #settings summary::after { content: ' ▾'; }
        #settings[open] summary::after { content: ' ▴'; }
        .scale-row {
            display: flex; align-items: center; gap: 10px;
            width: 260px; margin-top: 14px;
        }
        .scale-row label {
            font-size: 0.72rem; color: #555; letter-spacing: 1px;
            text-transform: uppercase; width: 42px; text-align: right;
            flex-shrink: 0;
        }
        .scale-row input[type=range] {
            flex: 1; accent-color: #1a6bcc; cursor: pointer;
        }
        .scale-row span {
            font-size: 0.72rem; font-family: monospace; color: #555;
            width: 34px; text-align: left; flex-shrink: 0;
        }
    </style>
</head>
<body>
    <h1>Wifi Rover</h1>
    <div id="base">
        <div id="dz-x"></div>
        <div id="dz-y"></div>
        <div id="thumb"></div>
    </div>
    <details id="settings">
        <summary>Settings</summary>
        <div class="scale-row">
            <label>Speed</label>
            <input id="spd" type="range" min="0" max="100" value="100">
            <span id="spd-val">100%</span>
        </div>
        <div class="scale-row">
            <label>Steer</label>
            <input id="str" type="range" min="0" max="100" value="100">
            <span id="str-val">100%</span>
        </div>
    </details>
    <p id="status">Connecting...</p>
    <script>
        // Steering servo geometry (must match hardware config)
        const SERVO_LEFT   = 45;
        const SERVO_CENTER = 90;
        const SERVO_RIGHT  = 135;

        // Layout constants (must match CSS)
        const BASE_D  = 260;
        const THUMB_R = 36;
        const MAX_R   = BASE_D / 2 - THUMB_R;
        const DZ      = 0.18;
        const DZ_PX   = Math.round(MAX_R * DZ);
        const PCT     = 50 * MAX_R / (BASE_D / 2);

        document.getElementById('dz-x').style.cssText =
            `top:0;bottom:0;left:${BASE_D/2 - DZ_PX}px;width:${DZ_PX*2}px;`;
        document.getElementById('dz-y').style.cssText =
            `left:0;right:0;top:${BASE_D/2 - DZ_PX}px;height:${DZ_PX*2}px;`;

        const base   = document.getElementById('base');
        const thumb  = document.getElementById('thumb');
        const status = document.getElementById('status');
        const spdSlider = document.getElementById('spd');
        const strSlider = document.getElementById('str');
        const spdVal    = document.getElementById('spd-val');
        const strVal    = document.getElementById('str-val');
        spdSlider.addEventListener('input', () => { spdVal.innerText = spdSlider.value + '%'; transmit(true); });
        strSlider.addEventListener('input', () => { strVal.innerText = strSlider.value + '%'; transmit(true); });

        // WebSocket
        let ws;
        function connect() {
            ws = new WebSocket('ws://' + location.host + '/ws');
            ws.onopen  = () => { status.innerText = 'Ready'; };
            ws.onclose = () => {
                status.innerText = 'Disconnected – reconnecting…';
                setTimeout(connect, 1500);
            };
        }
        connect();
        function wsSend(c) { if (ws && ws.readyState === WebSocket.OPEN) ws.send(c); }

        // Joystick state
        let rawX = 0, rawY = 0, active = false;
        let lastCmd = null;

        function applyDz(v) {
            if (Math.abs(v) < DZ) return 0;
            return (v - Math.sign(v) * DZ) / (1 - DZ);
        }

        // Compute hardware values from joystick position
        function compute() {
            const spdScale = spdSlider.value / 100;
            const strScale = strSlider.value / 100;
            const motor = applyDz(rawY) * 100 * spdScale;
            const steer = applyDz(rawX) * strScale;

            // Motor: forward channel / reverse channel
            const fwd = Math.round(Math.max(0, motor));
            const rev = Math.round(Math.max(0, -motor));

            // Servo angle from steer (-1..1) mapped to servo range
            const angle = Math.round(
                Math.max(SERVO_LEFT, Math.min(SERVO_RIGHT,
                    SERVO_CENTER + steer * (SERVO_RIGHT - SERVO_LEFT) / 2
                ))
            );

            return { fwd, rev, angle };
        }

        function updateUI() {
            thumb.style.left = (50 + rawX * PCT) + '%';
            thumb.style.top  = (50 - rawY * PCT) + '%';
            const my = applyDz(rawY);
            thumb.style.background =
                my >  0.05 ? '#22863a' :
                my < -0.05 ? '#b94e00' :
                             '#1a6bcc';
        }

        function transmit(force) {
            const { fwd, rev, angle } = compute();
            const cmd = fwd + ',' + rev + ',' + angle;
            if (!force && cmd === lastCmd) return;
            lastCmd = cmd;
            wsSend('c:' + cmd);
            status.innerText = 'F ' + fwd + '  R ' + rev + '  A ' + angle;
        }

        function onMove(e) {
            if (!active) return;
            const r  = base.getBoundingClientRect();
            const cx = r.left + r.width  / 2;
            const cy = r.top  + r.height / 2;
            let dx = Math.max(-MAX_R, Math.min(MAX_R, e.clientX - cx));
            let dy = Math.max(-MAX_R, Math.min(MAX_R, e.clientY - cy));
            rawX =  dx / MAX_R;
            rawY = -dy / MAX_R;
            updateUI();
            transmit();
        }

        function onStart(e) {
            e.preventDefault();
            active = true;
            base.setPointerCapture(e.pointerId);
            onMove(e);
        }

        function onEnd() {
            active = false;
            rawX = 0; rawY = 0;
            lastCmd = null;
            updateUI();
            wsSend('c:0,0,90');
            status.innerText = 'Ready';
        }

        base.addEventListener('pointerdown',   onStart);
        base.addEventListener('pointermove',   onMove);
        base.addEventListener('pointerup',     onEnd);
        base.addEventListener('pointercancel', onEnd);
    </script>
</body>
</html>
"#;

pub fn register_handlers(
    server: &mut EspHttpServer<'static>,
    servo: Arc<Mutex<LedcDriver<'static>>>,
    servo2: Arc<Mutex<LedcDriver<'static>>>,
    motors: Arc<Mutex<Motors<'static>>>,
) -> Result<()> {
    // Serve the control page
    server.fn_handler("/", Method::Get, |request| -> Result<()> {
        let mut response = request.into_ok_response()?;
        response.write_all(HTML_SITE.as_bytes())?;
        Ok(())
    })?;

    // WebSocket: receive pre-computed values and apply directly
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
                if let Some(vals) = cmd.strip_prefix("c:") {
                    let parts: Vec<&str> = vals.splitn(3, ',').collect();
                    if parts.len() == 3 {
                        let fwd = parts[0].parse::<u8>().unwrap_or(0).min(100);
                        let rev = parts[1].parse::<u8>().unwrap_or(0).min(100);
                        let angle = parts[2].parse::<u32>().unwrap_or(90).clamp(0, 180);
                        motors.lock().unwrap().drive(fwd, rev)?;
                        set_angle(&mut servo.lock().unwrap(), angle);
                        let mirrored = 180 - angle;
                        set_angle(&mut servo2.lock().unwrap(), mirrored);
                    }
                } else {
                    log::warn!("WS: unknown cmd '{}'", cmd);
                }
            }
            Ok((frame_type, _)) => { log::debug!("WS: unhandled frame {:?}", frame_type); }
            Err(e) => { log::warn!("WS recv error: {:?}", e); }
        }
        Ok(())
    })?;

    Ok(())
}
