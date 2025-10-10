/// Upgrade an existing requirement to a new version
pub fn upgrade_requirement(req: &str, version: &semver::Version) -> anyhow::Result<Option<String>> {
    let req_text = req.to_string();
    let raw_req = semver::VersionReq::parse(&req_text)
        .expect("semver to generate valid version requirements");
    if raw_req.comparators.is_empty() {
        // Empty matches everything, no-change.
        Ok(None)
    } else {
        let comparators: anyhow::Result<Vec<_>> = raw_req
            .comparators
            .into_iter()
            .map(|p| set_comparator(p, version))
            .collect();
        let comparators = comparators?;
        let new_req = semver::VersionReq { comparators };
        let mut new_req_text = new_req.to_string();
        if new_req_text.starts_with('^') && !req.starts_with('^') {
            new_req_text.remove(0);
        }
        if new_req_text == req_text {
            Ok(None)
        } else {
            Ok(Some(new_req_text))
        }
    }
}

fn set_comparator(
    mut pred: semver::Comparator,
    version: &semver::Version,
) -> anyhow::Result<semver::Comparator> {
    match pred.op {
        semver::Op::Wildcard => {
            pred.major = version.major;
            if pred.minor.is_some() {
                pred.minor = Some(version.minor);
            }
            if pred.patch.is_some() {
                pred.patch = Some(version.patch);
            }
            Ok(pred)
        }
        semver::Op::Exact | semver::Op::Tilde | semver::Op::Caret => {
            Ok(assign_partial_req(version, pred))
        }
        _ => {
            let user_pred = pred.to_string();
            Err(unsupported_version_req(&user_pred))
        }
    }
}

fn assign_partial_req(
    version: &semver::Version,
    mut pred: semver::Comparator,
) -> semver::Comparator {
    pred.major = version.major;
    if pred.minor.is_some() {
        pred.minor = Some(version.minor);
    }
    if pred.patch.is_some() {
        pred.patch = Some(version.patch);
    }
    pred.pre = version.pre.clone();
    pred
}

fn unsupported_version_req(req: &str) -> anyhow::Error {
    anyhow::anyhow!("Support for modifying {req} is currently unsupported")
}
