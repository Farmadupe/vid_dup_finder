use std::path::PathBuf;

use itertools::Itertools;
use rayon::prelude::*;

use crate::{
    app::run_gui,
    library::{CacheCfg, SearchCfg, Tolerance},
};

mod fudan_preproc;

#[test]
fn run_fudan_tests() {
    let annotations = fudan_preproc::preprocess().unwrap();

    let _desired_video = PathBuf::from("/mnt/ssd-luks/fudan_dataset/processed/titanic_fly_scene/1803c61a647e94580b6b4fa9dd0677ae787df1fb_00:00:00-00:02:45.mp4");

    //let annotations = annotations.into_iter().filter(|annotation| annotation.iter_videos().any(|video| video.processed_path() == &desired_video)).collect::<Vec<_>>();

    let annotations_len = annotations.len();

    let cache_cfg = CacheCfg {
        cache_dir: PathBuf::from("/mnt/ssd-luks/phash-output-trimmed"),
        no_refresh_caches: false,
        debug_reload_errors: false,
        debug_reload_non_videos: true,
    };

    let load_search_cfg = SearchCfg {
        cand_dirs: vec![PathBuf::from("/mnt/ssd-luks/fudan_dataset/processed")],
        ref_dirs: vec![],
        excl_dirs: vec![],
        vec_search: true,
        determ: true,
        affirm_matches: false,
        tolerance: Tolerance {
            spatial: 0.15,
            temporal: 0.15,
        },
        cartesian: false,
    };

    crate::app::configure_logs(false, true);

    let cache = crate::library::load_disk_caches(&cache_cfg).unwrap();

    crate::library::update_dct_cache_from_fs(&cache, &load_search_cfg).unwrap();

    cache.save().unwrap();

    let num_matches = std::sync::atomic::AtomicU64::new(0);
    let thunks_n_outputs = annotations.par_iter().enumerate().filter_map(|(i, annotation)| {
        let (annotation_vid_a, annotation_vid_b) = annotation.iter_videos().tuple_windows::<(_, _)>().next().unwrap();

        let search_search_cfg = SearchCfg {
            cand_dirs: vec![
                annotation_vid_a.processed_path().to_path_buf(),
                annotation_vid_b.processed_path().to_path_buf(),
            ],
            ..load_search_cfg.clone()
        };

        let (output, _) = crate::library::find_all_matches(&cache, &cache_cfg, &search_search_cfg).unwrap();

        //println!("{}", output.len());

        let matched = output.len() == 1;

        let mut resolution_thunks = output.create_resolution_thunks(&cache);

        crate::app::sort_thunks(&mut resolution_thunks);

        if matched {
            num_matches.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let output = format!(
                "{}/{},{},{},{}",
                i,
                annotations_len,
                annotation_vid_a.processed_path().display(),
                annotation_vid_b.processed_path().display(),
                matched
            );
            Some((resolution_thunks, output))
        } else {
            None
        }
    });

    let thunks_n_outputs = thunks_n_outputs.collect::<Vec<_>>();
    let mut thunks = vec![];
    let mut outputs = vec![];

    for (thunk, output) in thunks_n_outputs {
        thunks.extend(thunk);
        outputs.push(output);
    }

    run_gui(thunks);

    for output in outputs {
        println!("{}", output);
    }

    println!(
        "annotations: {} matches: {}",
        annotations.len(),
        num_matches.into_inner()
    );
}
