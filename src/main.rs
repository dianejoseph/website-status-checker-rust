use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn main() -> Result<(), String> {
    // --- parse CLI args ---
    let mut args = std::env::args().skip(1);
    let mut file_arg: Option<String> = None;
    let mut workers: Option<usize> = None;
    let mut timeout_secs: Option<u64> = None;
    let mut retries: Option<u32> = None;
    let mut urls: Vec<String> = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--file" => {
                let path = args.next().ok_or("Expecting file path after --file")?;
                file_arg = Some(path);
            }
            "--workers" => {
                let w = args.next().ok_or("Expected number after --workers")?;
                workers = Some(w.parse::<usize>().map_err(|e| e.to_string())?);
            }
            "--timeout" => {
                let t = args.next().ok_or("Expected number after --timeout")?;
                timeout_secs = Some(t.parse::<u64>().map_err(|e| e.to_string())?);
            }
            "--retries" => {
                let r = args.next().ok_or("Expected number after --retries")?;
                retries = Some(r.parse::<u32>().map_err(|e| e.to_string())?);
            }
            other => {
                if other.starts_with('-') {
                    return Err(format!("Unknown argument: {}", other));
                }
                urls.push(other.to_string());
            }
        }
    }

    // --- load URLs from file if given ---
    if let Some(path) = file_arg {
        let f = File::open(&path)
            .map_err(|e| format!("Failed to open file {}: {}", path, e))?;
        for line in BufReader::new(f).lines() {
            let line = line.map_err(|e| format!("Error reading {}: {}", path, e))?;
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') { continue }
            urls.push(t.to_string());
        }
    }

    if urls.is_empty() {
        return Err("No URLs provided".into());
    }

    // --- config ---
    let num_workers = workers.unwrap_or_else(|| {
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
    });
    let timeout = Duration::from_secs(timeout_secs.unwrap_or(5));
    let max_retries = retries.unwrap_or(0);

    // --- HTTP client ---
    let client = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| format!("Failed to build client: {}", e))?;
    let client = Arc::new(client);

    // --- channels ---
    let (task_tx, task_rx) = mpsc::channel::<String>();
    let (result_tx, result_rx) =
        mpsc::channel::<(String, Option<u16>, Option<String>, u128, u128)>();
    let task_rx = Arc::new(Mutex::new(task_rx));

    // --- spawn workers ---
    let mut handles = Vec::new();
    for _ in 0..num_workers {
        let task_rx = Arc::clone(&task_rx);
        let result_tx = result_tx.clone();
        let client = Arc::clone(&client);

        handles.push(thread::spawn(move || {
            while let Ok(url) = task_rx.lock().unwrap().recv() {
                let start = Instant::now();
                let mut status = None;
                let mut error = None;

                for i in 0..=max_retries {
                    match client.get(&url).send() {
                        Ok(r) => {
                            status = Some(r.status().as_u16());
                            break;
                        }
                        Err(_e) if i < max_retries => {
                            thread::sleep(Duration::from_millis(100));
                        }
                        Err(e) => {
                            error = Some(e.to_string());
                        }
                    }
                }

                let elapsed_ms = start.elapsed().as_millis();
                let ts = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as u128;

                let _ = result_tx.send((url.clone(), status, error, elapsed_ms, ts));
            }
        }));
    }
    drop(result_tx);

    // --- send tasks ---
    for u in urls {
        task_tx.send(u).map_err(|e| e.to_string())?;
    }
    drop(task_tx);

    // --- collect & print ---
    let mut results: Vec<(String, Option<u16>, Option<String>, u128, u128)> = Vec::new();
    for (url, st, err, ms, ts) in result_rx {
        if let Some(code) = st {
            println!("{} – status: {} – time: {}ms", url, code, ms);
        } else if let Some(e) = &err {
            println!("{} – error: {} – time: {}ms", url, e, ms);
        }
        results.push((url, st, err, ms, ts));
    }

    // --- clean up threads ---
    for h in handles {
        h.join().map_err(|_| "Thread panicked".to_string())?;
    }

    // --- write JSON ---
    let mut out = File::create("status.json").map_err(|e| e.to_string())?;
    writeln!(out, "[").map_err(|e| e.to_string())?;
    for (i, (url, st, err, ms, ts)) in results.iter().enumerate() {
        writeln!(out, "  {{").map_err(|e| e.to_string())?;
        writeln!(out, "    \"url\": \"{}\",", url).map_err(|e| e.to_string())?;
        if let Some(code) = st {
            writeln!(out, "    \"status_code\": {},", code).map_err(|e| e.to_string())?;
            writeln!(out, "    \"error\": null,").map_err(|e| e.to_string())?;
        } else {
            writeln!(out, "    \"status_code\": null,").map_err(|e| e.to_string())?;
            let efield = err.as_ref().map(|s| format!("\"{}\"", s)).unwrap_or("null".into());
            writeln!(out, "    \"error\": {},", efield).map_err(|e| e.to_string())?;
        }
        writeln!(out, "    \"response_time_ms\": {},", ms).map_err(|e| e.to_string())?;
        writeln!(out, "    \"timestamp\": {}", ts).map_err(|e| e.to_string())?;
        writeln!(out, "  }}{}", if i + 1 == results.len() { "" } else { "," }).map_err(|e| e.to_string())?;
    }
    writeln!(out, "]").map_err(|e| e.to_string())?;

    Ok(())
}
