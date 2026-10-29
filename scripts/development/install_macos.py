#!/usr/bin/env python3
"""Install a local Hub bundle without killing processes or discarding the old app."""
import argparse
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import uuid


def validate_bundle(bundle):
    if bundle.is_symlink() or not bundle.is_dir():
        raise ValueError(f"应用必须是普通目录：{bundle}")
    for part in (bundle / "Contents", bundle / "Contents/MacOS"):
        if part.is_symlink() or not part.is_dir():
            raise ValueError(f"应用结构必须是普通目录：{part}")
    info = bundle / "Contents/Info.plist"
    if info.is_symlink():
        raise ValueError("Info.plist 不能是符号链接")
    with info.open("rb") as stream:
        metadata = plistlib.load(stream)
    if not isinstance(metadata, dict):
        raise ValueError("应用 Info.plist 必须是字典")
    if metadata.get("CFBundleIdentifier") != "com.cataclysmhub.desktop":
        raise ValueError("应用标识不匹配，拒绝替换")
    executable = metadata.get("CFBundleExecutable")
    if not isinstance(executable, str) or Path(executable).name != executable or executable in ("", ".", ".."):
        raise ValueError("应用入口无效")
    binary = bundle / "Contents/MacOS" / executable
    if binary.is_symlink() or not binary.is_file() or not os.access(binary, os.X_OK):
        raise ValueError("应用入口不存在或不可执行")


def assert_stopped():
    result = subprocess.run(["/usr/bin/pgrep", "-x", "cataclysm-hub"], capture_output=True, text=True)
    if result.returncode == 0:
        raise RuntimeError("检测到 Cataclysm Hub 正在运行，请正常退出后重试；不会自动结束进程")
    if result.returncode != 1:
        raise RuntimeError(f"无法检查应用进程：{result.stderr.strip()}")


def copy_bundle(source, target):
    subprocess.run(["/usr/bin/ditto", str(source), str(target)], check=True)


def install(source, directory):
    if sys.platform != "darwin":
        raise RuntimeError("此安装脚本仅支持 macOS")
    source = Path(source).absolute()
    directory = Path(directory).expanduser().absolute()
    validate_bundle(source)
    target = directory / "Cataclysm Hub.app"
    if target.is_symlink() or directory.is_symlink():
        raise ValueError("安装目录或目标应用不能是符号链接")
    # Resolve aliases before checking overlap, including a symlink in an ancestor.
    src, dst = source.resolve(), directory.resolve()
    if src == dst or src in dst.parents or target.resolve() == src or target.resolve() in src.parents:
        raise ValueError("源应用与安装目标不能相同或嵌套")
    assert_stopped()
    if target.exists():
        validate_bundle(target)
    directory.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix=".cataclysm-hub-install-", dir=directory))
    backup = None
    try:
        prepared = stage / "Cataclysm Hub.app"
        copy_bundle(source, prepared)
        validate_bundle(prepared)
        assert_stopped()
        if target.is_symlink():
            raise ValueError("安装目标已变为符号链接")
        if target.exists():
            validate_bundle(target)
            backup = directory / f"Cataclysm Hub.previous-{uuid.uuid4().hex}.app"
            target.rename(backup)
        try:
            prepared.rename(target)
        except OSError:
            if backup is not None:
                try:
                    backup.rename(target)
                except OSError as error:
                    raise RuntimeError(f"替换失败且无法恢复旧路径；旧应用保留于：{backup}") from error
            raise
    finally:
        shutil.rmtree(stage)
    return target, backup


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--destination", type=Path, default=Path.home() / "Applications")
    args = parser.parse_args()
    try:
        target, backup = install(args.source, args.destination)
    except (OSError, ValueError, RuntimeError, plistlib.InvalidFileException, subprocess.CalledProcessError) as error:
        print(f"安装失败：{error}", file=sys.stderr)
        return 1
    print(f"已安装：{target}")
    if backup:
        print(f"旧应用保留于：{backup}")
    print("未启动应用；游戏、存档和 Hub 元数据未迁移。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
