from pathlib import Path
import sys
source = Path(sys.argv[1]).read_text()
start = source.index('        let mut inheriting_packages = Vec::new();')
end = source.index('        let workspace_version_pkgs:', start)
block = source[start:end]
candidate_start = source.rfind('        let (workspace_version, is_workspace_root) =', 0, start)
if candidate_start != -1:
    root = source[candidate_start:start]
    assert root.strip().endswith('};'), root
    block = root + block
else:
    function_start = source.index('    fn new_workspace_version(')
    root_start = source.index('        let workspace_version = {', function_start)
    root_end = source.index('        };', root_start) + len('        };\n')
    root = source[root_start:root_end]
    block += root
# The wrapper receives a borrowed slice rather than owning the prebuilt Vec.
block = block.replace('for package in &workspace_packages {', 'for package in workspace_packages {')
out = '''use cargo_metadata::{Package, camino::Utf8Path, semver::Version};
use cargo_utils::LocalManifest;

// Generated from the production inheritance-discovery block, including the
// workspace-version read previously performed by new_workspace_version.
fn inheritance_scan<'a>(workspace_packages: &'a [Package], local_manifest_path: &Utf8Path)
    -> anyhow::Result<(Option<Version>, Vec<&'a Package>)> {
'''+block+'''    Ok((workspace_version, inheriting_packages))
}
'''
Path(sys.argv[2]).write_text(out)
