use hound::{self, WavSpec};
use rustfft::{FftPlanner, num_complex::Complex};
use std::collections::HashMap;

// Wavファイルを読み込み、窓関数を適用したデータを返す
fn get_wave(path: &str) -> Result<(WavSpec, Vec<f64>), Box<dyn std::error::Error>> {
    let mut target = hound::WavReader::open(path)?;

    let spec = target.spec();

    let samples = match spec.sample_format {
        hound::SampleFormat::Int => {
            match spec.bits_per_sample {
                16 => {
                    target
                        .samples::<i16>()
                        .map(|s| s.unwrap() as f64 / (1 << (spec.bits_per_sample - 1)) as f64)
                        .collect::<Vec<f64>>()
                },
                _ => {
                    target
                        .samples::<i32>()
                        .map(|s| s.unwrap() as f64 / (1 << (spec.bits_per_sample - 1)) as f64)
                        .collect::<Vec<f64>>()
                }
            }
        },
        hound::SampleFormat::Float => {
            // 32bit floatの場合
            target
                .samples::<f32>()
                .map(|s| s.unwrap() as f64)
                .collect::<Vec<f64>>()
        },
    };

    // ハミング窓を適用
    let hamming_window: Vec<f64> = (0..samples.len())
        .map(|i| {
            0.54 - 0.46 * (2.0 * std::f64::consts::PI * i as f64 / samples.len() as f64).cos()
        }).collect();
    let samples: Vec<f64> = samples.iter().zip(hamming_window.iter()).map(|(x, y)| x * y).collect();

    Ok((spec, samples))
}


fn get_note(freq: f64) -> String {
    let notes = vec![
        "A", "A#", "B", "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#"
    ];

    let octave = (4f64 + (freq / 440.0).log2()) as i32;
    let note = notes[(((4f64 + (freq / 440.0).log2()) * 12f64).round() as usize) % 12];
    format!("{}{}", note, octave)
}

// ピークからコードを解析
fn analyze_chord(peaks: Vec<f64>) -> String {
    // ルート音の周波数を取得
    let root_freq = peaks.iter().fold(0.0/0.0, |m, v| v.min(m));

    // ルート音より1オクターブ高い音を除去
    let peaks: Vec<f64> = peaks.iter().filter(|x| **x < root_freq * 2.0).map(|x| *x).collect();

    // ルート音からの相対音程を取得
    let mut distances: Vec<i32> = peaks.iter().map(|x| ((x / root_freq).log2() * 12f64).round() as i32).collect();

    distances.sort();
    distances.dedup();

    // ルート音からの相対音程とコード名のハッシュマップ
    let chord_map: HashMap<Vec<i32>, String> =  {
        let mut chord_map: HashMap<Vec<i32>, String> =  HashMap::new();
        chord_map.insert(vec![0, 4, 7], "major".to_string());
        chord_map.insert(vec![0, 3, 7], "minor".to_string());
        chord_map.insert(vec![0, 4, 7, 10], "seventh".to_string());
        chord_map.insert(vec![0, 4, 7, 11], "major_seventh".to_string());
        chord_map.insert(vec![0, 3, 7, 10], "minor_seventh".to_string());
        chord_map.insert(vec![0, 3, 7, 11], "minor_major_seventh".to_string());
        chord_map.insert(vec![0, 4, 8], "augmented".to_string());
        chord_map.insert(vec![0, 3, 6], "diminished".to_string());
        chord_map.insert(vec![0, 3, 6, 9], "diminished_seventh".to_string());
        chord_map.insert(vec![0, 3, 6, 10], "minor_seventh_flat_five".to_string());

        chord_map
    };

    // 音程からコード名を取得
    let name = match chord_map.get(&distances) {
        Some(name) => name.clone(),
        // 未知のコードは全て単音として扱う
        None => "".to_string(),
    };

    // ルート音の音名とコード名を結合
    format!("{} {}", get_note(root_freq), name)
}


fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("ファイル名を入力してください: ");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();

    // 拡張子がない場合は補完する
    let path = if input.trim().contains(".") {
        format!("chords/{}", input.trim())
    } else {
        format!("chords/{}.wav", input.trim())
    };

    let Ok((spec, samples)) = get_wave(&path) else {
        println!("ファイルが見つかりません");
        return Ok(());
    };

    // FFTを実行
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(samples.len());

    let mut input: Vec<Complex<f64>> = samples.iter().map(|&x| Complex { re: x, im: 0.0 }).collect();

    fft.process(&mut input);

    let mut output: Vec<f64> = input.iter().map(|x| x.norm()).collect();

    // 出力物の範囲の右側を削除
    output.truncate(output.len() / 2);

    // 高周波ノイズを除去
    output.truncate(20000);

    // 波形を平滑化
    output = output.iter().zip(output.iter().skip(1)).zip(output.iter().skip(2)).map(|((x, y), z)| {
        (x + y + z) / 3.0
    }).collect();

    // ピークを取得
    let mut peaks: Vec<(usize, f64)> = output.iter().enumerate().zip(output.iter().skip(1)).zip(output.iter().skip(2)).filter_map(|(((i, x), y), z)| {
        // 一個手前と一個後ろの値より大きい場合にピークとして取得
        if y > x && y > z {
            Some((i, *y))
        } else {
            None
        }
    }).collect();

    // ピークの中から上位10個を取得
    peaks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    peaks.truncate(8);

    let main_freq: Vec<f64> = peaks.iter().map(|x| x.0 as f64 / input.len() as f64 * spec.sample_rate as f64).collect();

    println!("この音源のコードは {} です", analyze_chord(main_freq));
    Ok(())
}
