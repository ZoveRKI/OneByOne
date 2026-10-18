// 测试用的小工具：在系统临时目录里建一个 git 仓库，测试结束后删除
#![allow(dead_code)]

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

pub struct TempRepo {
    pub dir: PathBuf,
}

impl TempRepo {
    /// 新建并初始化一个 git 仓库
    pub fn new() -> Self {
        let repo = Self::empty();
        repo.git(&["init", "-q"]);
        repo
    }

    /// 只建目录，不初始化 git
    pub fn empty() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("onebyone-test-{}-{n}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).expect("创建临时目录失败");
        Self { dir }
    }

    /// 用 -c 临时指定身份和换行设置，不改动本机的 git 配置
    pub fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .args([
                "-c",
                "user.name=onebyone-test",
                "-c",
                "user.email=test@example.com",
            ])
            .args(["-c", "core.autocrlf=false", "-c", "init.defaultBranch=main"])
            .args(args)
            .current_dir(&self.dir)
            .output()
            .expect("运行 git 失败");
        assert!(
            out.status.success(),
            "git {args:?} 失败：{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    pub fn write(&self, rel: &str, content: impl AsRef<[u8]>) {
        let path = self.path(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("创建目录失败");
        }
        fs::write(path, content).expect("写入文件失败");
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.dir.join(rel)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        // git 会把对象文件设成只读，Windows 上要先去掉只读属性才能删除
        clear_readonly(&self.dir);
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[allow(clippy::permissions_set_readonly_false)]
fn clear_readonly(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            clear_readonly(&path);
        } else if let Ok(meta) = fs::metadata(&path) {
            let mut perms = meta.permissions();
            if perms.readonly() {
                perms.set_readonly(false);
                let _ = fs::set_permissions(&path, perms);
            }
        }
    }
}
