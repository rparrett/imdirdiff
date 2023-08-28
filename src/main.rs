use image_compare::Similarity;
use std::{collections::HashSet, env, fmt::Display, fs, path::PathBuf, process};
use walkdir::WalkDir;
use yansi::Paint;

const IMAGE_EXTENSIONS: &[&str] = &["gif", "jpg", "jpeg", "png", "webp"];

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

    for subpath in images_a.difference(&images_b) {
        println!("[{}] {}", Paint::red("-"), subpath.display());
    }

    for subpath in images_b.difference(&images_a) {
        println!("[{}] {}", Paint::green("+"), subpath.display());
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

        if result.score < 1.0 {
            println!("[{}] {}", Paint::yellow("≠"), subpath.display());
        }
    }
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
}

impl Display for ImDirDiffError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::NotADirectory => write!(f, "Not a directory."),
            Self::DirIoError(ref e) => write!(f, "{}", e),
            Self::ImageError(ref e) => write!(f, "{}", e),
            Self::CompareError(ref e) => write!(f, "{}", e),
        }
    }
}
