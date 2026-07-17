use anyhow::{Context, Result, bail};
use html_escape::encode_text;
use regex::Regex;
use std::{
    cmp::Ordering,
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use urlencoding::encode;
use walkdir::WalkDir;

// ============================================================
// НАСТРОЙКИ
// ============================================================

/// Максимальное количество сертификатов в одной строке таблицы.
const MAX_COLUMNS: usize = 3;

/// Фиксированная ширина одной колонки сертификата.
const CERTIFICATE_COLUMN_WIDTH: usize = 250;

/// Папка с сертификатами.
const CERTIFICATES_DIRECTORY: &str = "certificates";

/// Генерируемые README-файлы.
const README_EN_PATH: &str = "README.md";
const README_RU_PATH: &str = "README_RU.md";

/// Ссылка на репозиторий.
const REPOSITORY_URL: &str = "https://github.com/TheAndreyZakharov/Certificates";

/// Поддерживаемые форматы изображений.
const SUPPORTED_EXTENSIONS: &[&str] = &["webp", "png", "jpg", "jpeg", "avif"];

// ============================================================
// МОДЕЛИ ДАННЫХ
// ============================================================

#[derive(Debug, Clone)]
struct CertificatePage {
    path: PathBuf,
    page_number: Option<u32>,
}

#[derive(Debug, Clone)]
struct Certificate {
    title: String,
    pages: Vec<CertificatePage>,
}

#[derive(Debug, Clone)]
struct Platform {
    name: String,
    certificates: Vec<Certificate>,
}

#[derive(Debug, Clone, Copy)]
enum Language {
    English,
    Russian,
}

impl Language {
    fn top_anchor(self) -> &'static str {
        match self {
            Language::English => "#certificates",
            Language::Russian => "#сертификаты",
        }
    }

    fn page_title(self) -> &'static str {
        match self {
            Language::English => "Certificates",
            Language::Russian => "Сертификаты",
        }
    }

    fn introduction(self) -> &'static str {
        match self {
            Language::English => {
                "This repository contains my certificates earned through \
                 courses, educational programs, professional training, \
                 and independent study."
            }
            Language::Russian => {
                "В этом репозитории собраны мои сертификаты, полученные \
                 за прохождение курсов, образовательных программ, \
                 профессионального обучения и самостоятельного изучения."
            }
        }
    }

    fn explanation(self) -> &'static str {
        match self {
            Language::English => {
                "The certificates are grouped by the platforms and organizations \
                 that issued them. Click an image to open the original file."
            }
            Language::Russian => {
                "Сертификаты сгруппированы по платформам и организациям, \
                 которые их выдали. Нажмите на изображение, чтобы открыть \
                 оригинальный файл."
            }
        }
    }

    fn total_certificates_label(self) -> &'static str {
        match self {
            Language::English => "Total certificates",
            Language::Russian => "Всего сертификатов",
        }
    }

    fn contents_label(self) -> &'static str {
        match self {
            Language::English => "Platforms and organizations",
            Language::Russian => "Платформы и организации",
        }
    }

    fn platform_certificates_label(self) -> &'static str {
        match self {
            Language::English => "Certificates",
            Language::Russian => "Сертификатов",
        }
    }

    fn total_pages_label(self) -> &'static str {
        match self {
            Language::English => "Total pages",
            Language::Russian => "Всего страниц",
        }
    }

    fn page_label(self) -> &'static str {
        match self {
            Language::English => "Page",
            Language::Russian => "Страница",
        }
    }

    fn back_to_top_label(self) -> &'static str {
        match self {
            Language::English => "Back to top",
            Language::Russian => "Наверх",
        }
    }
}

// ============================================================
// ЗАПУСК ПРОГРАММЫ
// ============================================================

fn main() -> Result<()> {
    validate_configuration()?;

    let certificates_root = Path::new(CERTIFICATES_DIRECTORY);

    if !certificates_root.exists() {
        bail!(
            "Папка с сертификатами не найдена: {}",
            certificates_root.display()
        );
    }

    let platforms = scan_certificates(certificates_root)?;

    let total_certificates: usize = platforms
        .iter()
        .map(|platform| platform.certificates.len())
        .sum();

    let total_image_files: usize = platforms
        .iter()
        .flat_map(|platform| &platform.certificates)
        .map(|certificate| certificate.pages.len())
        .sum();

    if total_certificates == 0 {
        bail!(
            "В папке '{}' не найдено поддерживаемых изображений.",
            CERTIFICATES_DIRECTORY
        );
    }

    let english_readme = generate_readme(&platforms, total_certificates, Language::English);

    let russian_readme = generate_readme(&platforms, total_certificates, Language::Russian);

    fs::write(README_EN_PATH, english_readme)
        .with_context(|| format!("Не удалось записать {README_EN_PATH}"))?;

    fs::write(README_RU_PATH, russian_readme)
        .with_context(|| format!("Не удалось записать {README_RU_PATH}"))?;

    println!("README-файлы успешно созданы.");
    println!("Платформ: {}", platforms.len());
    println!("Сертификатов: {total_certificates}");
    println!("Файлов изображений: {total_image_files}");
    println!("Создан: {README_EN_PATH}");
    println!("Создан: {README_RU_PATH}");

    Ok(())
}

// ============================================================
// СКАНИРОВАНИЕ СЕРТИФИКАТОВ
// ============================================================

fn scan_certificates(root: &Path) -> Result<Vec<Platform>> {
    let page_suffix_regex = Regex::new(r"^(?P<title>.+)_(?P<page>[0-9]+)$")
        .context("Не удалось создать регулярное выражение")?;

    /*
        Сначала создаём все платформы,
        даже если они пустые.
        certificates/
        ├── Kaggle Learn/
        ├── Empty Platform/
        └── Coursera/
        Все три попадут в README.
    */
    let mut platforms_map: BTreeMap<String, Platform> = collect_platforms(root)?;

    /*
        Временная структура:
        Платформа
            Сертификат
                Страницы
    */
    let mut grouped: BTreeMap<String, BTreeMap<String, Vec<CertificatePage>>> = BTreeMap::new();

    for entry in WalkDir::new(root)
        .min_depth(2)
        .into_iter()
        .filter_entry(|entry| !is_hidden(entry.path()))
    {
        let entry = entry.context("Ошибка при чтении папки сертификатов")?;

        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();

        if !is_supported_image(path) {
            continue;
        }

        let relative_path = path
            .strip_prefix(root)
            .context("Ошибка получения относительного пути")?;

        let components = relative_path.components().collect::<Vec<_>>();

        if components.len() < 2 {
            continue;
        }

        let platform_name = components[0]
            .as_os_str()
            .to_string_lossy()
            .trim()
            .to_string();

        let file_stem = path
            .file_stem()
            .context("Нет имени файла")?
            .to_string_lossy()
            .trim()
            .to_string();

        let (certificate_title, page_number) =
            parse_certificate_name(&file_stem, &page_suffix_regex)?;

        grouped
            .entry(platform_name)
            .or_default()
            .entry(certificate_title)
            .or_default()
            .push(CertificatePage {
                path: path.to_path_buf(),
                page_number,
            });
    }

    /*
        Добавляем найденные сертификаты
        в уже существующие платформы.
    */
    for (platform_name, certificates) in grouped {
        let platform = platforms_map
            .entry(platform_name.clone())
            .or_insert(Platform {
                name: platform_name.clone(),
                certificates: Vec::new(),
            });

        for (certificate_name, mut pages) in certificates {
            pages.sort_by(compare_certificate_pages);

            platform.certificates.push(Certificate {
                title: certificate_name,
                pages,
            });
        }

        platform.certificates.sort_by(compare_certificates);
    }

    let mut platforms = platforms_map.into_values().collect::<Vec<_>>();

    platforms.sort_by(compare_platforms);

    Ok(platforms)
}

fn collect_platforms(root: &Path) -> Result<BTreeMap<String, Platform>> {
    let mut platforms = BTreeMap::new();

    for entry in
        fs::read_dir(root).with_context(|| format!("Не удалось прочитать {}", root.display()))?
    {
        let entry = entry?;

        let path = entry.path();

        /*
            Берём только папки:

            certificates/
                Platform 1/
                Platform 2/

            Файлы игнорируются.
        */
        if !path.is_dir() {
            continue;
        }

        let name = path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .trim()
            .to_string();

        platforms.insert(
            name.clone(),
            Platform {
                name,
                certificates: Vec::new(),
            },
        );
    }

    Ok(platforms)
}

fn parse_certificate_name(
    file_stem: &str,
    page_suffix_regex: &Regex,
) -> Result<(String, Option<u32>)> {
    if let Some(captures) = page_suffix_regex.captures(file_stem) {
        let title = captures
            .name("title")
            .context("Не удалось определить название сертификата")?
            .as_str()
            .trim()
            .to_string();

        let page_number = captures
            .name("page")
            .context("Не удалось определить номер страницы")?
            .as_str()
            .parse::<u32>()
            .with_context(|| format!("Некорректный номер страницы в имени файла: {file_stem}"))?;

        return Ok((title, Some(page_number)));
    }

    Ok((file_stem.trim().to_string(), None))
}

fn compare_certificate_pages(left: &CertificatePage, right: &CertificatePage) -> Ordering {
    match (left.page_number, right.page_number) {
        (Some(left_number), Some(right_number)) => left_number.cmp(&right_number),

        (None, Some(_)) => Ordering::Less,

        (Some(_), None) => Ordering::Greater,

        (None, None) => natural_path_key(&left.path).cmp(&natural_path_key(&right.path)),
    }
}

fn compare_certificates(left: &Certificate, right: &Certificate) -> Ordering {
    left.title
        .to_lowercase()
        .cmp(&right.title.to_lowercase())
        .then_with(|| left.title.cmp(&right.title))
}

fn compare_platforms(left: &Platform, right: &Platform) -> Ordering {
    left.name
        .to_lowercase()
        .cmp(&right.name.to_lowercase())
        .then_with(|| left.name.cmp(&right.name))
}

// ============================================================
// ГЕНЕРАЦИЯ README
// ============================================================

fn generate_readme(
    platforms: &[Platform],
    total_certificates: usize,
    language: Language,
) -> String {
    let mut output = String::new();

    output.push_str(&generate_header(language));

    output.push_str(&generate_introduction(language, total_certificates));

    output.push_str(&generate_contents(platforms, language));

    for platform in platforms {
        output.push_str(&generate_platform_section(platform, language));
    }

    output
}

// ============================================================
// ШАПКА README
// ============================================================

fn generate_header(language: Language) -> String {
    let (russian_color, english_color) = match language {
        Language::English => ("blue", "brightgreen"),
        Language::Russian => ("brightgreen", "blue"),
    };

    format!(
        r#"<div align="center">

# {title}

[![Русский](https://img.shields.io/badge/README_Language-Русский-{russian_color})]({repository}/blob/main/README_RU.md)
[![English](https://img.shields.io/badge/README_Language-English-{english_color})]({repository}/blob/main/README.md)

</div>

"#,
        title = language.page_title(),
        russian_color = russian_color,
        english_color = english_color,
        repository = REPOSITORY_URL,
    )
}

// ============================================================
// ВВЕДЕНИЕ И ОБЩЕЕ КОЛИЧЕСТВО
// ============================================================

fn generate_introduction(language: Language, total_certificates: usize) -> String {
    format!(
        r#"{introduction}

{explanation}

<div align="center">

## {total_label}

# {total_certificates}

</div>

---

"#,
        introduction = language.introduction(),
        explanation = language.explanation(),
        total_label = language.total_certificates_label(),
    )
}

// ============================================================
// ОГЛАВЛЕНИЕ
// ============================================================

fn generate_contents(platforms: &[Platform], language: Language) -> String {
    let mut output = String::new();

    output.push_str(&format!("## {}\n\n", language.contents_label(),));

    for platform in platforms {
        output.push_str(&format!(
            "- [{}](#{}) — {}\n",
            escape_markdown_text(&platform.name),
            github_heading_anchor(&platform.name),
            platform.certificates.len(),
        ));
    }

    output.push_str("\n---\n\n");

    output
}

// ============================================================
// СЕКЦИЯ ПЛАТФОРМЫ
// ============================================================

fn generate_platform_section(platform: &Platform, language: Language) -> String {
    let mut output = String::new();

    output.push_str(&format!(
        "## {}\n\n",
        escape_markdown_heading(&platform.name),
    ));

    output.push_str("<div align=\"center\">\n\n");

    output.push_str(&format!(
        "**{}: {}**\n\n",
        language.platform_certificates_label(),
        platform.certificates.len(),
    ));

    output.push_str("</div>\n\n");

    output.push_str(&generate_platform_table(platform, language));

    output.push_str(&format!(
        "<p align=\"right\"><a href=\"{}\">↑ {}</a></p>\n\n",
        language.top_anchor(),
        encode_text(language.back_to_top_label()),
    ));

    output.push_str("---\n\n");

    output
}

// ============================================================
// ЦЕНТРИРОВАННЫЕ ТАБЛИЦЫ ПЛАТФОРМЫ
// ============================================================

fn generate_platform_table(platform: &Platform, language: Language) -> String {
    let mut output = String::new();

    let image_width = configured_image_width();

    /*
        Каждая строка сертификатов — отдельная таблица.

        Так строки с одним или двумя сертификатами имеют ширину
        ровно по содержимому и центрируются целиком, а длинные
        названия не растягивают соседние изображения.
    */
    for row in platform.certificates.chunks(MAX_COLUMNS) {
        output.push_str(&generate_certificate_row_table(row, image_width, language));
    }

    output
}

fn generate_certificate_row_table(
    certificates: &[Certificate],
    image_width: usize,
    language: Language,
) -> String {
    let mut output = String::new();

    if certificates.is_empty() {
        return output;
    }

    let table_width = certificates.len() * CERTIFICATE_COLUMN_WIDTH;

    output.push_str(&format!(
        "<table align=\"center\" width=\"{table_width}\">\n",
    ));
    output.push_str("<tbody>\n");

    output.push_str("<tr>\n");
    for certificate in certificates {
        output.push_str(&generate_certificate_title_cell(certificate));
    }
    output.push_str("</tr>\n");

    output.push_str("<tr>\n");
    for certificate in certificates {
        output.push_str(&format!(
            "<td width=\"{width}\" align=\"center\" valign=\"top\" style=\"width: {width}px; min-width: {width}px; max-width: {width}px;\">\n",
            width = CERTIFICATE_COLUMN_WIDTH,
        ));

        output.push_str(&generate_certificate_images(
            certificate,
            image_width,
            language,
        ));

        output.push_str("</td>\n");
    }
    output.push_str("</tr>\n");

    output.push_str("</tbody>\n");
    output.push_str("</table>\n\n");
    output
}

// ============================================================
// ЯЧЕЙКИ СЕРТИФИКАТА
// ============================================================

fn generate_certificate_title_cell(certificate: &Certificate) -> String {
    let mut output = String::new();

    output.push_str(&format!(
        "<td width=\"{width}\" align=\"center\" valign=\"top\" style=\"width: {width}px; min-width: {width}px; max-width: {width}px; overflow-wrap: anywhere; word-break: break-word;\">\n",
        width = CERTIFICATE_COLUMN_WIDTH,
    ));

    output.push_str(&format!(
        "<strong>{}</strong>\n",
        encode_text(&certificate.title),
    ));

    output.push_str("</td>\n");
    output
}

fn generate_certificate_images(
    certificate: &Certificate,
    image_width: usize,
    language: Language,
) -> String {
    let mut output = String::new();

    /*
        Для многостраничного сертификата сверху выводится:

        Всего страниц: 5
        Total pages: 5
    */
    if certificate.pages.len() > 1 {
        output.push_str(&format!(
            "<sub><strong>{}: {}</strong></sub><br><br>\n",
            encode_text(language.total_pages_label()),
            certificate.pages.len(),
        ));
    }

    for (index, page) in certificate.pages.iter().enumerate() {
        let encoded_path = encode_repository_path(&page.path);

        let displayed_page_number = page.page_number.unwrap_or((index + 1) as u32);

        let alt_text = if certificate.pages.len() > 1 {
            format!(
                "{} — {} {}",
                certificate.title,
                language.page_label(),
                displayed_page_number,
            )
        } else {
            certificate.title.clone()
        };

        output.push_str(&format!(
            "<a href=\"{path}\"><img src=\"{path}\" alt=\"{alt}\" width=\"{width}\"></a>\n",
            path = encoded_path,
            alt = encode_text(&alt_text),
            width = image_width,
        ));

        /*
            Под каждой страницей многостраничного сертификата:

            Страница 1
            Page 1
        */
        if certificate.pages.len() > 1 {
            output.push_str(&format!(
                "<br><sub>{} {}</sub>\n",
                encode_text(language.page_label()),
                displayed_page_number,
            ));
        }

        if index + 1 < certificate.pages.len() {
            output.push_str("<br><br>\n");
        }
    }

    output
}

// ============================================================
// ОДИНАКОВАЯ ШИРИНА ВСЕХ КОЛОНОК И ИЗОБРАЖЕНИЙ
// ============================================================

fn configured_image_width() -> usize {
    CERTIFICATE_COLUMN_WIDTH
}

// ============================================================
// ПУТИ И ЭКРАНИРОВАНИЕ
// ============================================================

fn encode_repository_path(path: &Path) -> String {
    path.components()
        .map(|component| {
            let component_text = component.as_os_str().to_string_lossy();

            encode(&component_text).into_owned()
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn escape_markdown_text(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn escape_markdown_heading(value: &str) -> String {
    value.replace('\n', " ").trim().to_string()
}

fn github_heading_anchor(value: &str) -> String {
    let lower = value.to_lowercase();

    let mut anchor = String::new();
    let mut previous_was_hyphen = false;

    for character in lower.chars() {
        if character.is_alphanumeric() || character == '_' {
            anchor.push(character);
            previous_was_hyphen = false;
        } else if character.is_whitespace() || character == '-' {
            if !anchor.is_empty() && !previous_was_hyphen {
                anchor.push('-');
                previous_was_hyphen = true;
            }
        }
    }

    while anchor.ends_with('-') {
        anchor.pop();
    }

    anchor
}

fn natural_path_key(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase()
}

// ============================================================
// ПРОВЕРКИ
// ============================================================

fn validate_configuration() -> Result<()> {
    if MAX_COLUMNS == 0 {
        bail!("MAX_COLUMNS должен быть больше нуля.");
    }

    if configured_image_width() == 0 {
        bail!("Рассчитанная ширина изображения должна быть больше нуля.");
    }

    if REPOSITORY_URL.trim().is_empty() {
        bail!("REPOSITORY_URL не должен быть пустым.");
    }

    Ok(())
}

fn is_supported_image(path: &Path) -> bool {
    let Some(extension) = path.extension() else {
        return false;
    };

    let extension = extension.to_string_lossy().to_lowercase();

    SUPPORTED_EXTENSIONS.contains(&extension.as_str())
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}
