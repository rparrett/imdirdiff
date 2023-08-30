use image_compare::Similarity;
use std::{collections::HashSet, env, fmt::Display, fs, io::Write, path::PathBuf, process};
use walkdir::WalkDir;
use yansi::Paint;

const IMAGE_EXTENSIONS: &[&str] = &["gif", "jpg", "jpeg", "png", "webp"];
const GENERATE_REPORT: bool = true;
const REPORT_PATH: &str = "./imdirdiff-out";

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
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: imdirdiff dir1 dir2");
        process::exit(1);
    }

    let path_a = PathBuf::from(&args[1]);
    let path_b = PathBuf::from(&args[2]);

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
        let image_path_a = [path_a.clone(), subpath.clone()].iter().collect();
        let image_path_b = [path_b.clone(), subpath.clone()].iter().collect();

        let result = compare(&image_path_a, &image_path_b);
        let result = match result {
            Err(e) => {
                eprintln!("Error comparing images: {}", e);
                process::exit(1);
            }
            Ok(r) => r,
        };

        if GENERATE_REPORT {
            if let Err(e) = copy_report_image(&image_path_a, subpath, "a") {
                eprintln!("Error copying report image: {}", e);
                process::exit(1);
            }
            if let Err(e) = copy_report_image(&image_path_b, subpath, "b") {
                eprintln!("Error copying report image: {}", e);
                process::exit(1);
            }

            let image_path_diff: PathBuf =
                [PathBuf::from(REPORT_PATH), "diff".into(), subpath.clone()]
                    .iter()
                    .collect();

            if let Err(e) = fs::create_dir_all(image_path_diff.clone().with_file_name("")) {
                eprintln!("Error creating diff image: {}", e);
                process::exit(1);
            }

            if let Err(e) = result.image.to_color_map().save(image_path_diff.clone()) {
                eprintln!("{}: {}", e, image_path_diff.display());
                process::exit(1);
            }
        }

        if result.score < 1.0 {
            let result = DiffResult {
                diff: Diff::Different {
                    similarity: result.score,
                },
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

fn copy_report_image(
    path: &PathBuf,
    subpath: &PathBuf,
    prefix: &str,
) -> Result<(), ImDirDiffError> {
    let report_image_a: PathBuf = [PathBuf::from(REPORT_PATH), prefix.into(), subpath.clone()]
        .iter()
        .collect();
    fs::create_dir_all(report_image_a.clone().with_file_name(""))
        .map_err(ImDirDiffError::ReportIoError)?;
    fs::copy(path, report_image_a).map_err(ImDirDiffError::ReportIoError)?;

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
        "<style>img {{ max-height: 80px; }} body {{ columns: 3; }}</style>"
    )
    .map_err(ImDirDiffError::ReportIoError)?;

    for result in results {
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
                write!(
                    &mut report,
                    "<div>
                        {}
                        <div>
                            <a href=\"a/{}\"><img loading=\"lazy\" src=\"a/{}\"></a>
                            <a href=\"b/{}\"><img loading=\"lazy\" src=\"b/{}\"></a>
                            <a href=\"diff/{}\"><img loading=\"lazy\" src=\"diff/{}\"></a>
                        </div>
                    </div>",
                    result.path.display(),
                    result.path.display(),
                    result.path.display(),
                    result.path.display(),
                    result.path.display(),
                    result.path.display(),
                    result.path.display(),
                )
                .map_err(ImDirDiffError::ReportIoError)?;
            }
        }
    }

    Ok(())
}

fn compare(image_path_a: &PathBuf, image_path_b: &PathBuf) -> Result<Similarity, ImDirDiffError> {
    let image_a = image::open(image_path_a)
        .map_err(ImDirDiffError::ImageError)?
        .into_rgb8();

    let image_b = image::open(image_path_b)
        .map_err(ImDirDiffError::ImageError)?
        .into_rgb8();

    image_compare::rgb_hybrid_compare(&image_a, &image_b).map_err(ImDirDiffError::CompareError)
}

fn relative_image_paths(dir_path: &PathBuf) -> HashSet<PathBuf> {
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

fn check_dir(dir_path: &PathBuf) -> Result<(), ImDirDiffError> {
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
    ReportIoError(std::io::Error),
}

impl Display for ImDirDiffError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::NotADirectory => write!(f, "Not a directory."),
            Self::DirIoError(ref e) => write!(f, "{}", e),
            Self::ImageError(ref e) => write!(f, "{}", e),
            Self::CompareError(ref e) => write!(f, "{}", e),
            Self::ReportIoError(ref e) => write!(f, "{}", e),
        }
    }
}
