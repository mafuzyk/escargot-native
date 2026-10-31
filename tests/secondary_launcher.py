"""Verify the second-account launcher isolates account files and preserves assets."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parents[1]
launcher = root / "msn2"
assert launcher.is_file(), "A second-account launcher must be available to open another session"
with tempfile.TemporaryDirectory(prefix="secondary-launcher-", dir="/dev/shm") as directory:
    fixture = Path(directory)
    shutil.copyfile(launcher, fixture / "msn2")
    (fixture / "escargot-native").symlink_to(sys.executable)
    primary_data = fixture / "primary-data"
    primary_config = fixture / "primary-config"
    env = dict(os.environ, XDG_CONFIG_HOME=str(primary_config), XDG_DATA_HOME=str(primary_data))
    env.pop("ESCARGOT_NATIVE_ASSETS", None)
    payload = "import json,os,sys; print(json.dumps({k:os.environ.get(k) for k in ['XDG_CONFIG_HOME','XDG_DATA_HOME','ESCARGOT_NATIVE_ASSETS']} | {'args':sys.argv[1:]}))"
    result = subprocess.run(["/bin/sh", str(fixture / "msn2"), "-c", payload, "test argument"], env=env, capture_output=True, text=True, check=True)
    settings = json.loads(result.stdout)
    profile = primary_data / "escargot-native-secondary"
    assert settings["XDG_CONFIG_HOME"] == str(profile / "config"), "The second account must not reuse the primary settings"
    assert settings["XDG_DATA_HOME"] == str(profile / "data"), "The second account must have separate history and credentials files"
    assert settings["ESCARGOT_NATIVE_ASSETS"] == str(primary_data / "escargot-native"), "Both accounts must use the installed artwork"
    assert settings["args"] == ["test argument"], "The launcher must preserve arguments"
    assert not primary_config.exists() and not primary_data.exists(), "Launching must not copy or modify the primary account files"
print("Second-account launcher isolates settings and history, and shares the installed assets")
