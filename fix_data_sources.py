import os
import re

for root, dirs, files in os.walk('src'):
    for file in files:
        if file.endswith('.rs'):
            path = os.path.join(root, file)
            with open(path, 'r') as f:
                content = f.read()
            
            # Find `impl DataSource for ... {`
            if 'impl DataSource for ' in content:
                # Replace `fn source(path: PathType) -> Result<&'static str> {`
                new_content = re.sub(
                    r'fn source\((.*?): PathType\) -> Result<&\'static str> {',
                    r'fn source(\1: PathType, version: Option<&semver::Version>) -> Result<String> {',
                    content
                )
                
                # We need to replace `Ok("some_string")` or `Ok("some_string".to_string())`?
                # The compiler will complain about mismatched types if we just change the signature and not `Ok("foo")` -> `Ok("foo".to_string())`.
                # But it's easier to just do a simple replacement: `Ok("...")` -> `Ok("...".to_string())`
                # Let's do that for lines inside the source function.
                
                # A better approach: we will change all `Ok("...")` in the file that are returned by `source`
                # Let's just run sed to replace `Ok("([^"]+)")` -> `Ok("\1".to_string())` globally in lines that have `Ok("` and are in a match arm.
                
                # For `src/processor/elasticsearch/`, we want to use `get_source`!
                # Except for manifest files... Wait, manifest files are in `src/processor/diagnostic/`.
                
                if 'src/processor/elasticsearch/' in path:
                    # For ES, we rewrite the `source` method completely to use `get_source`.
                    class_match = re.search(r'impl DataSource for (\w+)', content)
                    if class_match:
                        class_name = class_match.group(1)
                        print(f"Rewriting ES data source in {path}")
                        
                        # Replace the whole fn source block
                        # We can find `fn source` to the end of its block by counting braces.
                        # But it's simpler to just do:
                        replacement = '''fn source(path: PathType, version: Option<&semver::Version>) -> Result<String> {
        let name = Self::name();
        if let Ok(source_conf) = crate::processor::diagnostic::data_source::get_source(Self::product(), &name) {
            match path {
                PathType::File => Ok(source_conf.get_file_path(&name)),
                PathType::Url => {
                    let v = version.ok_or_else(|| eyre::eyre!("Version required for URL"))?;
                    source_conf.get_url(v)
                }
            }
        } else {
            // Fallback for missing or not-yet-supported sources
            eyre::bail!("Source configuration missing for product: {}, name: {}", Self::product(), name)
        }
    }'''
                        
                        new_content = re.sub(
                            r'fn source\([^\}]*\}\s*\}',
                            replacement,
                            content,
                            flags=re.DOTALL
                        )
                else:
                    # For logstash and diagnostic manifests, just fix the types
                    new_content = re.sub(
                        r'Ok\("([^"]+)"\)',
                        r'Ok("\1".to_string())',
                        new_content
                    )
                
                if content != new_content:
                    with open(path, 'w') as f:
                        f.write(new_content)
