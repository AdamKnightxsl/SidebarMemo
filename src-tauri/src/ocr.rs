//! PaddleOCR 模块 — 基于 ocr-rs (PP-OCRv6 + MNN 推理)
//! 精度远超 Windows.Media.Ocr，支持中英文，无需 Python / Tesseract
//! 模型文件随项目打包分发，位于 src-tauri/models/

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug)]
pub struct OcrWord {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub confidence: f32,
    // ── 真实墨水范围（原图像素坐标，由灰度投影测量得到）──
    // DB 检测框经 unclip 扩张比墨水大 ~1.56 倍，前端优先用 ink 字段渲染
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ink_x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ink_y: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ink_width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ink_height: Option<f64>,
}

#[derive(Serialize, Clone, Debug)]
pub struct OcrOutput {
    pub words: Vec<OcrWord>,
    pub width: f64,
    pub height: f64,
}

/// 字符类型（用于拆分边界判断）
#[derive(PartialEq, Clone, Copy)]
enum CharClass {
    Cjk,     // 中日韩文字
    Digit,   // 数字
    Alpha,   // 英文字母
    Space,   // 空格
    Punct,   // 标点/其他
}

fn char_class(c: char) -> CharClass {
    if c.is_ascii_digit() {
        CharClass::Digit
    } else if c.is_ascii_alphabetic() {
        CharClass::Alpha
    } else if c == ' ' || c == '\u{3000}' {
        CharClass::Space
    } else if is_cjk(c) {
        CharClass::Cjk
    } else {
        CharClass::Punct
    }
}

fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}' |   // CJK 统一表意文字
        '\u{3400}'..='\u{4DBF}' |   // CJK 扩展 A
        '\u{F900}'..='\u{FAFF}' |   // CJK 兼容表意
        '\u{3000}'..='\u{303F}' |   // CJK 标点
        '\u{FF00}'..='\u{FFEF}'     // 全角字符
    )
}

/// 计算字符串的显示宽度单位（基于实际字体渲染宽度比例）
/// CJK = 1.0（全角基准）
/// 数字 = 0.69（数字字符宽 ≈ 0.55em，但 inkRatio 低导致 font-size 更大，综合 ≈ 0.69）
/// 字母 = 0.55（ASCII 字母平均宽度）
fn char_units(s: &str) -> f64 {
    s.chars().map(|c| {
        if is_cjk(c) { 1.0 }
        else if c.is_ascii_digit() { 0.69 }
        else if c.is_ascii_alphabetic() { 0.55 }
        else { 0.5 }
    }).sum()
}

/// 把一行 OCR 文本拆分为多个段（按字符类型边界）
/// 空格保留为独立的间距段，确保原始布局中的空白距离不丢失。
/// 例如: "文字A                 文字B" → ["文字A", "                 ", "文字B"]
/// 例如: "订单编号6928235329501428781下单时间" → ["订单编号", "6928235329501428781", "下单时间"]
fn split_into_segments(text: &str) -> Vec<OcrWord> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return vec![];
    }

    let mut segments: Vec<OcrWord> = Vec::new();
    let mut current = String::new();
    let mut current_class = char_class(chars[0]);

    for &c in &chars {
        let cc = char_class(c);
        // 拆分规则：类型变化时断开
        let should_split = match (current_class, cc) {
            (_, CharClass::Space) => true,       // 空格前断开
            (CharClass::Space, _) => true,       // 空格后断开
            (CharClass::Cjk, CharClass::Digit) => true,
            (CharClass::Digit, CharClass::Cjk) => true,
            (CharClass::Cjk, CharClass::Alpha) => true,
            (CharClass::Alpha, CharClass::Cjk) => true,
            (CharClass::Digit, CharClass::Alpha) => true,
            (CharClass::Alpha, CharClass::Digit) => true,
            _ => false,
        };

        if should_split && !current.is_empty() {
            segments.push(OcrWord {
                text: current.clone(),
                x: 0.0, y: 0.0, width: 0.0, height: 0.0, confidence: 0.0,
                ink_x: None, ink_y: None, ink_width: None, ink_height: None,
            });
            current.clear();
        }

        // 关键修复：保留空格字符，使其参与宽度分配，保持原始间距
        current.push(c);
        current_class = cc;
    }

    if !current.is_empty() {
        segments.push(OcrWord {
            text: current,
            x: 0.0, y: 0.0, width: 0.0, height: 0.0, confidence: 0.0,
            ink_x: None, ink_y: None, ink_width: None, ink_height: None,
        });
    }

    segments
}

/// 模型文件名（PP-OCRv6 small — 精度与速度的最佳平衡）
const DET_MODEL: &str = "PP-OCRv6_small_det.mnn";
const REC_MODEL: &str = "PP-OCRv6_small_rec.mnn";
const CHARSET_FILE: &str = "ppocr_keys_v6_small.txt";

/// 确保模型文件存在，返回三个模型文件路径
fn resolve_models(models_dir: &Path) -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let det = models_dir.join(DET_MODEL);
    let rec = models_dir.join(REC_MODEL);
    let keys = models_dir.join(CHARSET_FILE);

    if !det.exists() || !rec.exists() || !keys.exists() {
        return Err(format!(
            "OCR 模型文件缺失，请确认 models 目录存在: {}",
            models_dir.display()
        ));
    }

    Ok((det, rec, keys))
}

/// 在灰度图的指定 bbox 区域内测量真实墨水范围（原图像素坐标）
/// DB 检测框经 unclip 扩张比真实文字大 ~1.56 倍，此函数用 Otsu 二值化 + 投影法
/// 找出文字实际占据的像素范围，供前端做墨水级精确对齐。
/// 返回 Some((ink_x, ink_y, ink_w, ink_h))，失败/退化返回 None
fn compute_ink_bbox(gray: &image::GrayImage, x: f64, y: f64, w: f64, h: f64) -> Option<(f64, f64, f64, f64)> {
    let (img_w, img_h) = (gray.width() as i64, gray.height() as i64);
    // 1. bbox 坐标 clamp 到图像边界
    let x0 = (x.floor() as i64).clamp(0, img_w);
    let y0 = (y.floor() as i64).clamp(0, img_h);
    let x1 = ((x + w).ceil() as i64).clamp(0, img_w);
    let y1 = ((y + h).ceil() as i64).clamp(0, img_h);
    let rw = (x1 - x0) as usize;
    let rh = (y1 - y0) as usize;
    if rw < 3 || rh < 3 {
        return None;
    }

    // 2. 统计区域内 256 级灰度直方图，Otsu 法求自适应阈值（类间方差最大化）
    let mut hist = [0u64; 256];
    for yy in y0..y1 {
        for xx in x0..x1 {
            hist[gray.get_pixel(xx as u32, yy as u32)[0] as usize] += 1;
        }
    }
    let total = (rw * rh) as u64;
    let sum_all: u64 = hist.iter().enumerate().map(|(v, &c)| v as u64 * c).sum();
    let mut sum_b: u64 = 0; // 低灰度类的灰度加权和
    let mut w_b: u64 = 0;   // 低灰度类的像素数
    let mut best_var = -1.0f64;
    let mut threshold = 0usize;
    for t in 0..256 {
        w_b += hist[t];
        if w_b == 0 { continue; }
        let w_f = total - w_b;
        if w_f == 0 { break; }
        sum_b += t as u64 * hist[t];
        let m_b = sum_b as f64 / w_b as f64;
        let m_f = (sum_all - sum_b) as f64 / w_f as f64;
        let var_between = w_b as f64 * w_f as f64 * (m_b - m_f) * (m_b - m_f);
        if var_between > best_var {
            best_var = var_between;
            threshold = t;
        }
    }

    // 3. 极性判断：像素数较少的一类为墨水（文字在检测框内总是少数派，
    //    深底白字 / 浅底黑字均适用）；少数类占比过低或过高说明区分度差
    let dark: u64 = hist[..=threshold].iter().sum();
    let light = total - dark;
    let (ink_is_dark, ink_count) = if dark <= light { (true, dark) } else { (false, light) };
    let ratio = ink_count as f64 / total as f64;
    if ratio < 0.005 || ratio > 0.45 {
        return None;
    }

    // 判定单个像素是否为墨水
    let is_ink = |xx: i64, yy: i64| -> bool {
        let v = gray.get_pixel(xx as u32, yy as u32)[0] as usize;
        if ink_is_dark { v <= threshold } else { v > threshold }
    };

    // 4. 行投影：逐行统计墨水像素数，取最大连通有效行段
    //    （允许中间 ≤2 行断裂容忍，避免相邻行文字侵入 bbox 干扰）
    let min_row_ink = std::cmp::max(1, (rw as f64 * 0.02) as usize);
    let mut row_valid = vec![false; rh];
    for (ri, yy) in (y0..y1).enumerate() {
        let mut cnt = 0usize;
        for xx in x0..x1 {
            if is_ink(xx, yy) { cnt += 1; }
        }
        row_valid[ri] = cnt >= min_row_ink;
    }
    let mut best: Option<(usize, usize)> = None; // (ink_top, ink_bottom)
    let mut cur_start: Option<usize> = None;
    let mut cur_end = 0usize;
    let mut gap = 0usize;
    for ri in 0..rh {
        if row_valid[ri] {
            if cur_start.is_none() {
                cur_start = Some(ri);
            }
            cur_end = ri;
            gap = 0;
        } else if let Some(s) = cur_start {
            gap += 1;
            if gap > 2 {
                // 断裂超过容忍度，当前段结束，与已有最长段比较
                if best.map_or(true, |(bs, be)| cur_end - s > be - bs) {
                    best = Some((s, cur_end));
                }
                cur_start = None;
                gap = 0;
            }
        }
    }
    if let Some(s) = cur_start {
        if best.map_or(true, |(bs, be)| cur_end - s > be - bs) {
            best = Some((s, cur_end));
        }
    }
    let (ink_top, ink_bottom) = best?;

    // 5. 列投影：只统计 ink_top..=ink_bottom 范围内的行，取首尾有效列
    //    （列方向不取最大连通段，因为文字之间有天然空隙）
    let mut ink_left: Option<usize> = None;
    let mut ink_right = 0usize;
    for (ci, xx) in (x0..x1).enumerate() {
        let mut cnt = 0usize;
        for ri in ink_top..=ink_bottom {
            if is_ink(xx, y0 + ri as i64) { cnt += 1; }
        }
        if cnt >= 1 {
            if ink_left.is_none() { ink_left = Some(ci); }
            ink_right = ci;
        }
    }
    let ink_left = ink_left?;

    // 6. 转回原图坐标；过小的结果视为退化
    let ink_w = (ink_right - ink_left + 1) as f64;
    let ink_h = (ink_bottom - ink_top + 1) as f64;
    if ink_w < 2.0 || ink_h < 2.0 {
        return None;
    }
    Some((
        (x0 + ink_left as i64) as f64,
        (y0 + ink_top as i64) as f64,
        ink_w,
        ink_h,
    ))
}

/// 对指定图片执行 OCR，返回每个文字词的文本和边界框（原图像素坐标）
pub fn recognize(models_dir: &Path, memo_id: &str, filename: &str) -> Result<OcrOutput, String> {
    let path = dirs::data_dir()
        .ok_or("无法获取数据目录")?
        .join("sidebar-memo")
        .join("images")
        .join(memo_id)
        .join(filename);

    if !path.exists() {
        return Err("图片文件不存在".into());
    }

    // 解析模型文件路径
    let (det, rec, keys) = resolve_models(models_dir)?;

    // 加载图片
    let img = image::open(&path).map_err(|e| format!("加载图片失败: {}", e))?;
    let (img_w, img_h) = (img.width() as f64, img.height() as f64);

    // 创建 OCR 引擎（禁用文字框合并，保留原始间距）
    let mut det_opts = ocr_rs::DetOptions::new()
        .with_merge_boxes(false);
    det_opts.unclip_ratio = 1.0; // 收紧文字框，减少 bbox 扩张 padding

    let config = ocr_rs::OcrEngineConfig::new()
        .with_det_options(det_opts);

    let engine = ocr_rs::OcrEngine::new(
        det.to_str().ok_or("模型路径无效")?,
        rec.to_str().ok_or("模型路径无效")?,
        keys.to_str().ok_or("模型路径无效")?,
        Some(config),
    )
    .map_err(|e| format!("初始化 OCR 引擎失败: {}", e))?;

    let results = engine
        .recognize(&img)
        .map_err(|e| format!("OCR 识别失败: {}", e))?;

    // 整图一次性灰度化，供 ink bbox 测量复用（严禁在循环内重复转换）
    let gray = img.to_luma8();

    // 转换为前端需要的格式（拆分行级结果为词级，保留原始间距）
    let mut words: Vec<OcrWord> = Vec::new();
    for r in results {
        let rect = &r.bbox.rect;
        let x = rect.left() as f64;
        let y = rect.top() as f64;
        let w = rect.width() as f64;
        let h = rect.height() as f64;
        // 行级真实墨水范围（原图像素坐标）
        let ink = compute_ink_bbox(&gray, x, y, w, h);
        // 段级独立墨水测量：对段自身的 bbox 区域重新做投影测量。
        // 数字/符号段内部留白比例与中文不同，行级 ink 线性重映射会产生
        // 水平偏移和高度误差（数字墨水高度 ≠ 中文墨水高度），
        // 每段独立测量后互不影响。测量失败时回退行级 ink 线性重映射。
        let seg_ink = |seg_x: f64, seg_w: f64| -> (Option<f64>, Option<f64>, Option<f64>, Option<f64>) {
            if let Some((ix, iy, iw, ih)) = compute_ink_bbox(&gray, seg_x, y, seg_w, h) {
                return (Some(ix), Some(iy), Some(iw), Some(ih));
            }
            match ink {
                Some((ink_x0, ink_y0, ink_w0, ink_h0)) if w > 0.0 => (
                    Some(ink_x0 + (seg_x - x) / w * ink_w0),
                    Some(ink_y0),
                    Some(seg_w / w * ink_w0),
                    Some(ink_h0),
                ),
                _ => (None, None, None, None),
            }
        };
        let segments = split_into_segments(&r.text);
        let conf = r.confidence;
        if segments.len() <= 1 {
            let (ink_x, ink_y, ink_width, ink_height) = match ink {
                Some((ix, iy, iw, ih)) => (Some(ix), Some(iy), Some(iw), Some(ih)),
                None => (None, None, None, None),
            };
            words.push(OcrWord {
                text: r.text, x, y, width: w, height: h, confidence: conf,
                ink_x, ink_y, ink_width, ink_height,
            });
        } else {
            // 分类段：文字段 vs 空格段
            let text_segs: Vec<(usize, &OcrWord)> = segments.iter().enumerate()
                .filter(|(_, s)| !s.text.chars().all(|c| c == ' ' || c == '\u{3000}'))
                .collect();
            let space_seg_indices: Vec<usize> = segments.iter().enumerate()
                .filter(|(_, s)| s.text.chars().all(|c| c == ' ' || c == '\u{3000}'))
                .map(|(i, _)| i)
                .collect();

            // 计算文字内容应占宽度（CJK=2单位，ASCII=1单位）
            let text_units: f64 = text_segs.iter().map(|(_, s)| char_units(&s.text)).sum();
            // 空格段的“权重”：每个空格字符计为 1 个单位（与 ASCII 字符等宽）
            // 这样 17 个空格的段会获得 17 倍于 1 个空格的宽度
            let total_space_chars: f64 = space_seg_indices.iter()
                .map(|&i| segments[i].text.chars().count() as f64)
                .sum();
            let text_width = if !space_seg_indices.is_empty() {
                // 有空格段：文字和空格按各自单位数比例分配 bbox 宽度
                (text_units / (text_units + total_space_chars)) * w
            } else {
                // 无空格段：文字占满整个 bbox 宽度（不添加额外间隙）
                w
            };
            let gap_width_total = w - text_width;

            // 给每个文字段分配宽度
            let unit_px = text_width / text_units.max(1.0);
            let mut seg_widths: Vec<f64> = vec![0.0; segments.len()];
            for &(idx, seg) in &text_segs {
                seg_widths[idx] = char_units(&seg.text) * unit_px;
            }

            // 剩余宽度分配给空格段
            if !space_seg_indices.is_empty() {
                // 有空格段：剩余宽度按空格字符数比例分配
                for &idx in &space_seg_indices {
                    let space_chars = segments[idx].text.chars().count() as f64;
                    seg_widths[idx] = gap_width_total * (space_chars / total_space_chars.max(1.0));
                }
            } else if text_segs.len() > 1 {
                // 无空格但多段（类型边界拆分）：在 CJK↔数字/字母 边界添加小间隙
                let mut boundary_count = 0;
                for i in 1..text_segs.len() {
                    let prev_c = text_segs[i - 1].1.text.chars().last().unwrap();
                    let curr_c = text_segs[i].1.text.chars().next().unwrap();
                    let (pc, cc) = (char_class(prev_c), char_class(curr_c));
                    if matches!((pc, cc),
                        (CharClass::Cjk, CharClass::Digit) | (CharClass::Digit, CharClass::Cjk) |
                        (CharClass::Cjk, CharClass::Alpha) | (CharClass::Alpha, CharClass::Cjk))
                    {
                        boundary_count += 1;
                    }
                }
                // 每个边界间隙 ≈ 0.5 个 CJK 字符宽度（= 1.0 单位）
                let gap_per_boundary = if boundary_count > 0 {
                    (w * 0.02).min(w / (text_units + boundary_count as f64) * 0.5)
                } else { 0.0 };
                let total_gap = gap_per_boundary * boundary_count as f64;
                let remaining_w = w - total_gap;
                let unit_px = remaining_w / text_units.max(1.0);
                for &(idx, seg) in &text_segs {
                    seg_widths[idx] = char_units(&seg.text) * unit_px;
                }

                let mut cur_x = x;
                for i in 0..text_segs.len() {
                    let (idx, seg) = text_segs[i];
                    let (ink_x, ink_y, ink_width, ink_height) = seg_ink(cur_x, seg_widths[idx]);
                    words.push(OcrWord {
                        text: seg.text.clone(),
                        x: cur_x,
                        y,
                        width: seg_widths[idx],
                        height: h,
                        confidence: conf,
                        ink_x, ink_y, ink_width, ink_height,
                    });
                    cur_x += seg_widths[idx];
                    // 在 CJK↔数字/字母 边界插入间隙
                    if i + 1 < text_segs.len() {
                        let curr_c = seg.text.chars().last().unwrap();
                        let next_c = text_segs[i + 1].1.text.chars().next().unwrap();
                        let (cc, nc) = (char_class(curr_c), char_class(next_c));
                        if matches!((cc, nc),
                            (CharClass::Cjk, CharClass::Digit) | (CharClass::Digit, CharClass::Cjk) |
                            (CharClass::Cjk, CharClass::Alpha) | (CharClass::Alpha, CharClass::Cjk))
                        {
                            cur_x += gap_per_boundary;
                        }
                    }
                }
                continue; // 已处理，跳过下方通用逻辑
            }

            // 通用输出：按顺序遍历所有段，跳过空格段但保留其宽度推进
            let mut cur_x = x;
            for (idx, seg) in segments.iter().enumerate() {
                let seg_w = seg_widths[idx];
                let is_space = seg.text.chars().all(|c| c == ' ' || c == '\u{3000}');
                if !is_space {
                    let (ink_x, ink_y, ink_width, ink_height) = seg_ink(cur_x, seg_w);
                    words.push(OcrWord {
                        text: seg.text.clone(),
                        x: cur_x,
                        y,
                        width: seg_w,
                        height: h,
                        confidence: conf,
                        ink_x, ink_y, ink_width, ink_height,
                    });
                }
                cur_x += seg_w; // 无论是否空格，都推进 x 坐标
            }
        }
    }

    // 调试：打印前 8 个识别结果（含真实墨水范围）
    for (i, w) in words.iter().take(8).enumerate() {
        let ink_info = match (w.ink_x, w.ink_y, w.ink_width, w.ink_height) {
            (Some(ix), Some(iy), Some(iw), Some(ih)) =>
                format!("ink=({:.0},{:.0}) {:.0}x{:.0}", ix, iy, iw, ih),
            _ => "ink=None".to_string(),
        };
        eprintln!("[OCR]   #{}: \"{}\" at ({:.0},{:.0}) {:.0}x{:.0} {}", i, w.text, w.x, w.y, w.width, w.height, ink_info);
    }
    eprintln!("[OCR] 拆分后共 {} 个文字区域", words.len());

    Ok(OcrOutput {
        words,
        width: img_w,
        height: img_h,
    })
}
