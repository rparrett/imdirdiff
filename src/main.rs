use argh::FromArgs;
use image::imageops::{overlay, FilterType};
use regex::Regex;
use std::{
    collections::HashSet,
    fmt::Display,
    fs,
    io::Write,
    path::Path,
    path::PathBuf,
    process::{self, Command},
};
use walkdir::WalkDir;
use yansi::Paint;

const IMAGE_EXTENSIONS: &[&str] = &["gif", "jpg", "jpeg", "png", "webp"];
const GENERATE_REPORT: bool = true;
const REPORT_PATH: &str = "./imdirdiff-out";
const THUMB_WIDTH: u32 = u32::MAX;
const THUMB_HEIGHT: u32 = 80;
const THUMB_EXTENSION: &str = "sm.jpg";

static RE_FLIP: once_cell::sync::Lazy<Regex> =
    once_cell::sync::Lazy::new(|| Regex::new(r"Mean: ([\d.]+)").unwrap());

#[derive(FromArgs)]
/// Reach new heights.
struct Args {
    /// use nvidia's flip (https://github.com/NVlabs/flip) instead of image_compare
    #[argh(switch)]
    flip: bool,
    #[argh(positional)]
    a: String,
    #[argh(positional)]
    b: String,
}

enum Diff {
    OnlyInA,
    OnlyInB,
    Different { similarity: f64 },
}

struct DiffResult {
    diff: Diff,
    path: PathBuf,
}

fn main() {
    let args: Args = argh::from_env();

    let path_a = PathBuf::from(&args.a);
    let path_b = PathBuf::from(&args.b);

    if let Err(e) = check_dir(&path_a) {
        eprintln!("Error reading {}: {}", path_a.display(), e);
        process::exit(1);
    }

    if let Err(e) = check_dir(&path_b) {
        eprintln!("Error reading {}: {}", path_b.display(), e);
        process::exit(1);
    }

    let images_a = relative_image_paths(&path_a);
    let images_b = relative_image_paths(&path_b);

    let mut results = vec![];

    for subpath in images_a.difference(&images_b) {
        let result = DiffResult {
            path: subpath.clone(),
            diff: Diff::OnlyInA,
        };
        print_result(&result);
        results.push(result);
    }

    for subpath in images_b.difference(&images_a) {
        let result = DiffResult {
            path: subpath.clone(),
            diff: Diff::OnlyInB,
        };
        print_result(&result);
        results.push(result);
    }

    for subpath in images_a.intersection(&images_b) {
        let similarity = if args.flip {
            compare_flip(&path_a, &path_b, subpath)
        } else {
            compare(&path_a, &path_b, subpath)
        };

        let similarity = match similarity {
            Err(e) => {
                eprintln!("Error comparing {} {}", subpath.display(), e);
                process::exit(1);
            }
            Ok(r) => r,
        };

        if similarity < 1.0 {
            let result = DiffResult {
                diff: Diff::Different { similarity },
                path: subpath.clone(),
            };
            print_result(&result);
            results.push(result);
        }
    }

    if let Err(e) = generate_report(&results) {
        eprintln!("Error generating report: {}", e);
        process::exit(1);
    }
}

fn print_result(result: &DiffResult) {
    match result.diff {
        Diff::OnlyInA => {
            println!("[{}] {}", Paint::red("-"), result.path.display());
        }
        Diff::OnlyInB => {
            println!("[{}] {}", Paint::green("+"), result.path.display());
        }
        Diff::Different {
            similarity: _similarity,
        } => {
            println!("[{}] {}", Paint::yellow("≠"), result.path.display());
        }
    }
}

fn copy_report_image(path: &Path, subpath: &Path, prefix: &Path) -> Result<(), ImDirDiffError> {
    let report_image: PathBuf = [Path::new(REPORT_PATH), prefix, subpath].iter().collect();

    fs::create_dir_all(report_image.with_file_name("")).map_err(ImDirDiffError::ReportIoError)?;
    fs::copy(path, &report_image).map_err(ImDirDiffError::ReportIoError)?;

    let thumb_path = report_image.with_extension(THUMB_EXTENSION);

    let image = image::open(report_image).map_err(ImDirDiffError::ReportImageError)?;
    image.resize(u32::MAX, 80, FilterType::Triangle);
    image
        .save(thumb_path)
        .map_err(ImDirDiffError::ReportImageError)?;

    Ok(())
}

fn generate_report(results: &Vec<DiffResult>) -> Result<(), ImDirDiffError> {
    let index_path: PathBuf = [PathBuf::from(REPORT_PATH), "index.html".into()]
        .iter()
        .collect();
    let mut report = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(index_path)
        .map_err(ImDirDiffError::ReportIoError)?;

    write!(
        &mut report,
        "<style>{}</style>",
        include_str!("../templates/style.css")
    )
    .map_err(ImDirDiffError::ReportIoError)?;

    write!(&mut report, "<div>").map_err(ImDirDiffError::ReportIoError)?;

    for result in results {
        match result.diff {
            Diff::OnlyInA => {
                write!(
                    &mut report,
                    "<div>{} is only present in A</div>",
                    result.path.display()
                )
                .map_err(ImDirDiffError::ReportIoError)?;
            }
            Diff::OnlyInB => {
                write!(
                    &mut report,
                    "<div>{} is only present in B</div>",
                    result.path.display()
                )
                .map_err(ImDirDiffError::ReportIoError)?;
            }
            _ => {}
        }
    }

    write!(&mut report, "</div><div class=\"diffs\">").map_err(ImDirDiffError::ReportIoError)?;

    for result in results {
        let Diff::Different {
            similarity: _similarity,
        } = result.diff
        else {
            continue;
        };

        let thumb = result.path.with_extension(THUMB_EXTENSION);
        let thumb = thumb.display();
        let full_size = result.path.display();

        write!(
            &mut report,
            "<div class=\"diff\">
                {full_size} <span class=\"x\">x</span>
                <div>
                    <a href=\"a/{full_size}\"><img loading=\"lazy\" src=\"a/{thumb}\"></a>
                    <a href=\"b/{full_size}\"><img loading=\"lazy\" src=\"b/{thumb}\"></a>
                    <a href=\"diff/{full_size}\"><img loading=\"lazy\" src=\"diff/{thumb}\"></a>
                </div>
            </div>",
        )
        .map_err(ImDirDiffError::ReportIoError)?;
    }

    write!(&mut report, "</div>").map_err(ImDirDiffError::ReportIoError)?;

    write!(
        &mut report,
        "<script>{}</script>",
        include_str!("../templates/script.js")
    )
    .map_err(ImDirDiffError::ReportIoError)?;

    Ok(())
}

fn compare(path_a: &Path, path_b: &Path, subpath: &Path) -> Result<f64, ImDirDiffError> {
    let image_path_a: PathBuf = [path_a, subpath].iter().collect();
    let image_path_b: PathBuf = [path_b, subpath].iter().collect();

    let image_a = image::open(&image_path_a)
        .map_err(ImDirDiffError::ImageError)?
        .into_rgb8();

    let image_b = image::open(&image_path_b)
        .map_err(ImDirDiffError::ImageError)?
        .into_rgb8();

    let similarity = if image_a.dimensions() != image_b.dimensions() {
        let max_width = image_a.width().max(image_b.width());
        let max_height = image_a.height().max(image_b.height());

        let mut enlarged_a = image::ImageBuffer::new(max_width, max_height);
        overlay(&mut enlarged_a, &image_a, 0, 0);
        let mut enlarged_b = image::ImageBuffer::new(max_width, max_height);
        overlay(&mut enlarged_b, &image_b, 0, 0);

        image_compare::rgb_hybrid_compare(&enlarged_a, &enlarged_b)
            .map_err(ImDirDiffError::CompareError)?
    } else {
        image_compare::rgb_hybrid_compare(&image_a, &image_b)
            .map_err(ImDirDiffError::CompareError)?
    };

    if GENERATE_REPORT {
        copy_report_image(&image_path_a, subpath, Path::new("a"))?;
        copy_report_image(&image_path_b, subpath, Path::new("b"))?;

        let image_path_diff: PathBuf = [Path::new(REPORT_PATH), Path::new("diff"), subpath]
            .iter()
            .collect();

        fs::create_dir_all(image_path_diff.with_file_name(""))
            .map_err(ImDirDiffError::ReportIoError)?;

        let color_map = similarity.image.to_color_map();
        color_map
            .save(&image_path_diff)
            .map_err(ImDirDiffError::ReportImageError)?;

        color_map
            .resize(THUMB_WIDTH, THUMB_HEIGHT, FilterType::Triangle)
            .save(image_path_diff.with_extension(THUMB_EXTENSION))
            .map_err(ImDirDiffError::ReportImageError)?;
    }

    Ok(similarity.score)
}

fn compare_flip(path_a: &Path, path_b: &Path, subpath: &Path) -> Result<f64, ImDirDiffError> {
    let image_path_a: PathBuf = [path_a, subpath].iter().collect();
    let image_path_b: PathBuf = [path_b, subpath].iter().collect();

    let image_diff_dir: PathBuf = [
        Path::new(REPORT_PATH),
        Path::new("diff"),
        subpath.parent().unwrap(),
    ]
    .iter()
    .collect();

    // TODO I think it is possible to disable diff image saving if we are not generating
    // reports.

    let output = Command::new("flip")
        .args([
            "-r",
            image_path_a.to_str().unwrap(),
            "-t",
            image_path_b.to_str().unwrap(),
            "-d",
            image_diff_dir.to_str().unwrap(),
            "-b",
            subpath
                .with_extension("")
                .file_name()
                .unwrap()
                .to_str()
                .unwrap(),
        ])
        .output()
        .map_err(|_| ImDirDiffError::FlipError)?;

    let stdout =
        String::from_utf8(output.stdout).map_err(|_| ImDirDiffError::FlipOutputParseError)?;

    let caps = RE_FLIP
        .captures(&stdout)
        .ok_or(ImDirDiffError::FlipOutputParseError)?;

    let similarity: f64 = caps
        .get(1)
        .ok_or(ImDirDiffError::FlipOutputParseError)?
        .as_str()
        .parse()
        .map_err(|_| ImDirDiffError::FlipOutputParseError)?;

    if GENERATE_REPORT {
        copy_report_image(&image_path_a, subpath, Path::new("a"))?;
        copy_report_image(&image_path_b, subpath, Path::new("b"))?;

        let image_path_diff: PathBuf = [image_diff_dir, subpath.file_name().unwrap().into()]
            .iter()
            .collect();

        let image_diff = image::open(&image_path_diff).map_err(ImDirDiffError::ReportImageError)?;

        image_diff
            .resize(THUMB_WIDTH, THUMB_HEIGHT, FilterType::Triangle)
            .save(image_path_diff.with_extension(THUMB_EXTENSION))
            .map_err(ImDirDiffError::ReportImageError)?;
    }

    // TODO is this right? 0.0 is definitely "they are the same" but
    // I don't know what the maximum value is.
    Ok(1.0 - similarity)
}

fn relative_image_paths(dir_path: &Path) -> HashSet<PathBuf> {
    WalkDir::new(dir_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            let ext = path.extension()?;

            let ext = ext.to_str()?.to_lowercase();
            if !IMAGE_EXTENSIONS.contains(&ext.as_str()) {
                return None;
            }

            let relative = path.to_owned().strip_prefix(dir_path).unwrap().to_owned();

            Some(relative)
        })
        .collect()
}

fn check_dir(dir_path: &Path) -> Result<(), ImDirDiffError> {
    let meta = fs::metadata(dir_path).map_err(ImDirDiffError::DirIoError)?;
    if !meta.is_dir() {
        return Err(ImDirDiffError::NotADirectory);
    }

    Ok(())
}

enum ImDirDiffError {
    NotADirectory,
    DirIoError(std::io::Error),
    ImageError(image::ImageError),
    CompareError(image_compare::CompareError),
    FlipError,
    FlipOutputParseError,
    ReportIoError(std::io::Error),
    ReportImageError(image::ImageError),
}

impl Display for ImDirDiffError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::NotADirectory => write!(f, "Not a directory."),
            Self::DirIoError(ref e) => write!(f, "{}", e),
            Self::ImageError(ref e) => write!(f, "{}", e),
            Self::CompareError(ref e) => write!(f, "{}", e),
            Self::FlipError => write!(f, "Error running flip."),
            Self::FlipOutputParseError => write!(f, "Error parsing flip output."),
            Self::ReportIoError(ref e) => write!(f, "{}", e),
            Self::ReportImageError(ref e) => write!(f, "{}", e),
        }
    }
}
