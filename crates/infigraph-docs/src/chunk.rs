use crate::extract::ExtractedDoc;

#[derive(Debug, Clone)]
pub struct Chunk {
    pub id: String,
    pub doc_file: String,
    pub content_hash: String,
    pub index: usize,
    pub heading: Option<String>,
    pub text: String,
    pub start_offset: usize,
    pub end_offset: usize,
    pub page: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
pub enum ChunkStrategy {
    HeadingBounded,
    FixedToken { size: usize, overlap: usize },
}

impl ChunkStrategy {
    pub fn for_extension(ext: &str) -> Self {
        match ext {
            "md" | "markdown" | "rst" | "adoc" | "org" | "html" | "htm"
            | "xml" | "xsl" | "xsd" | "svg" | "plist" => Self::HeadingBounded,
            _ => Self::HeadingBounded,
        }
    }
}

pub fn chunk_document(doc: &ExtractedDoc, file: &str, hash: &str, strategy: ChunkStrategy) -> Vec<Chunk> {
    match strategy {
        ChunkStrategy::HeadingBounded => chunk_by_headings(doc, file, hash),
        ChunkStrategy::FixedToken { size, overlap } => chunk_by_tokens(doc, file, hash, size, overlap),
    }
}

const MAX_SECTION_TOKENS: usize = 512;
const SUB_CHUNK_OVERLAP: usize = 64;

fn chunk_by_headings(doc: &ExtractedDoc, file: &str, hash: &str) -> Vec<Chunk> {
    let text = &doc.text;
    if text.is_empty() {
        return Vec::new();
    }

    let heading_re = regex::Regex::new(r"(?m)^(#{1,6})\s+(.+)$|^([^\n]+)\n[=\-]{3,}$").unwrap();
    let mut sections: Vec<(Option<String>, usize, usize)> = Vec::new();
    let mut last_start = 0;
    let mut last_heading: Option<String> = None;

    for m in heading_re.find_iter(text) {
        if m.start() > last_start {
            sections.push((last_heading.clone(), last_start, m.start()));
        }
        last_start = m.start();
        let heading_text = m.as_str();
        last_heading = Some(
            heading_text
                .trim_start_matches('#')
                .trim()
                .lines()
                .next()
                .unwrap_or("")
                .to_string(),
        );
    }
    if last_start < text.len() {
        sections.push((last_heading, last_start, text.len()));
    }

    if sections.is_empty() {
        sections.push((None, 0, text.len()));
    }

    // No headings found → fall back to paragraph-bounded chunking
    if sections.len() == 1 && sections[0].0.is_none() {
        return chunk_by_paragraphs(doc, file, hash);
    }

    let mut chunks = Vec::new();
    let mut chunk_idx = 0;

    for (heading, start, end) in &sections {
        let section_text = text[*start..*end].trim();
        if section_text.is_empty() {
            continue;
        }

        let words: Vec<&str> = section_text.split_whitespace().collect();
        if words.len() <= MAX_SECTION_TOKENS {
            chunks.push(Chunk {
                id: format!("{}::chunk_{}", file, chunk_idx),
                doc_file: file.to_string(),
                content_hash: hash.to_string(),
                index: chunk_idx,
                heading: heading.clone(),
                text: section_text.to_string(),
                start_offset: *start,
                end_offset: *end,
                page: None,
            });
            chunk_idx += 1;
        } else {
            let mut w_start = 0;
            while w_start < words.len() {
                let w_end = (w_start + MAX_SECTION_TOKENS).min(words.len());
                let sub_text = words[w_start..w_end].join(" ");
                if !sub_text.is_empty() {
                    chunks.push(Chunk {
                        id: format!("{}::chunk_{}", file, chunk_idx),
                        doc_file: file.to_string(),
                        content_hash: hash.to_string(),
                        index: chunk_idx,
                        heading: heading.clone(),
                        text: sub_text,
                        start_offset: *start,
                        end_offset: *end,
                        page: None,
                    });
                    chunk_idx += 1;
                }
                if w_end >= words.len() {
                    break;
                }
                w_start = w_end - SUB_CHUNK_OVERLAP;
            }
        }
    }

    chunks
}

fn chunk_by_paragraphs(doc: &ExtractedDoc, file: &str, hash: &str) -> Vec<Chunk> {
    let text = &doc.text;
    if text.is_empty() {
        return Vec::new();
    }

    let paragraphs: Vec<&str> = text.split("\n\n")
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();

    if paragraphs.is_empty() {
        return chunk_by_tokens(doc, file, hash, MAX_SECTION_TOKENS, SUB_CHUNK_OVERLAP);
    }

    // If there's only one big block with no blank lines, fall back to fixed-token
    if paragraphs.len() == 1 {
        return chunk_by_tokens(doc, file, hash, MAX_SECTION_TOKENS, SUB_CHUNK_OVERLAP);
    }

    let mut chunks = Vec::new();
    let mut chunk_idx = 0;
    let mut current_text = String::new();
    let mut current_words = 0usize;
    let mut current_start = 0usize;

    for para in &paragraphs {
        let para_words = para.split_whitespace().count();

        // Single paragraph exceeds limit — flush current, then sub-chunk this paragraph
        if para_words > MAX_SECTION_TOKENS {
            if !current_text.is_empty() {
                let start_offset = text.find(current_text.trim()).unwrap_or(0);
                chunks.push(Chunk {
                    id: format!("{}::chunk_{}", file, chunk_idx),
                    doc_file: file.to_string(),
                    content_hash: hash.to_string(),
                    index: chunk_idx,
                    heading: infer_heading(current_text.trim()),
                    text: current_text.trim().to_string(),
                    start_offset,
                    end_offset: start_offset + current_text.trim().len(),
                    page: None,
                });
                chunk_idx += 1;
                current_text.clear();
                current_words = 0;
            }
            let words: Vec<&str> = para.split_whitespace().collect();
            let mut w_start = 0;
            while w_start < words.len() {
                let w_end = (w_start + MAX_SECTION_TOKENS).min(words.len());
                let sub_text = words[w_start..w_end].join(" ");
                let start_offset = text.find(&sub_text).unwrap_or(0);
                chunks.push(Chunk {
                    id: format!("{}::chunk_{}", file, chunk_idx),
                    doc_file: file.to_string(),
                    content_hash: hash.to_string(),
                    index: chunk_idx,
                    heading: infer_heading(&sub_text),
                    text: sub_text.clone(),
                    start_offset,
                    end_offset: start_offset + sub_text.len(),
                    page: None,
                });
                chunk_idx += 1;
                if w_end >= words.len() { break; }
                w_start = w_end - SUB_CHUNK_OVERLAP;
            }
            continue;
        }

        // Adding this paragraph would exceed limit — flush current chunk
        if current_words + para_words > MAX_SECTION_TOKENS && !current_text.is_empty() {
            let trimmed = current_text.trim();
            let start_offset = text[current_start..].find(trimmed)
                .map(|i| current_start + i)
                .unwrap_or(current_start);
            chunks.push(Chunk {
                id: format!("{}::chunk_{}", file, chunk_idx),
                doc_file: file.to_string(),
                content_hash: hash.to_string(),
                index: chunk_idx,
                heading: infer_heading(trimmed),
                text: trimmed.to_string(),
                start_offset,
                end_offset: start_offset + trimmed.len(),
                page: None,
            });
            chunk_idx += 1;
            current_text.clear();
            current_words = 0;
            current_start = text.find(para).unwrap_or(0);
        }

        if current_text.is_empty() {
            current_start = text.find(para).unwrap_or(0);
        }

        if !current_text.is_empty() {
            current_text.push_str("\n\n");
        }
        current_text.push_str(para);
        current_words += para_words;
    }

    // Flush remaining
    if !current_text.is_empty() {
        let trimmed = current_text.trim();
        let start_offset = text[current_start..].find(trimmed)
            .map(|i| current_start + i)
            .unwrap_or(current_start);
        chunks.push(Chunk {
            id: format!("{}::chunk_{}", file, chunk_idx),
            doc_file: file.to_string(),
            content_hash: hash.to_string(),
            index: chunk_idx,
            heading: infer_heading(trimmed),
            text: trimmed.to_string(),
            start_offset,
            end_offset: start_offset + trimmed.len(),
            page: None,
        });
    }

    chunks
}

fn infer_heading(text: &str) -> Option<String> {
    let first_line = text.lines().next().unwrap_or("").trim();
    if first_line.is_empty() {
        return None;
    }
    let words: Vec<&str> = first_line.split_whitespace().collect();
    // Short first line that looks like a title (under 10 words, no trailing punctuation)
    if words.len() <= 10 && !first_line.ends_with('.') && !first_line.ends_with(',') {
        Some(first_line.to_string())
    } else {
        None
    }
}

fn chunk_by_tokens(doc: &ExtractedDoc, file: &str, hash: &str, size: usize, overlap: usize) -> Vec<Chunk> {
    let text = &doc.text;
    if text.is_empty() {
        return Vec::new();
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut start = 0;
    let mut chunk_idx = 0;

    while start < words.len() {
        let end = (start + size).min(words.len());
        let chunk_text = words[start..end].join(" ");

        // Approximate byte offsets
        let start_offset = if start == 0 {
            0
        } else {
            text.find(words[start]).unwrap_or(0)
        };
        let end_offset = if end >= words.len() {
            text.len()
        } else {
            text.find(words[end.min(words.len() - 1)]).unwrap_or(text.len())
        };

        if !chunk_text.is_empty() {
            chunks.push(Chunk {
                id: format!("{}::chunk_{}", file, chunk_idx),
                doc_file: file.to_string(),
                content_hash: hash.to_string(),
                index: chunk_idx,
                heading: None,
                text: chunk_text,
                start_offset,
                end_offset,
                page: None,
            });
            chunk_idx += 1;
        }

        if end >= words.len() {
            break;
        }
        start = end - overlap;
    }

    chunks
}
