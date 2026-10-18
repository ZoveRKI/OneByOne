export interface DirNode {
  name: string;
  path: string;
  dirs: Map<string, DirNode>;
  files: string[];
}

/** 把文件路径列表整理成目录树 */
export function buildTree(paths: string[]): DirNode {
  const root: DirNode = { name: '', path: '', dirs: new Map(), files: [] };
  for (const p of paths) {
    const parts = p.split('/');
    let node = root;
    for (let i = 0; i < parts.length - 1; i++) {
      const name = parts[i] ?? '';
      let child = node.dirs.get(name);
      if (!child) {
        child = { name, path: parts.slice(0, i + 1).join('/'), dirs: new Map(), files: [] };
        node.dirs.set(name, child);
      }
      node = child;
    }
    node.files.push(p);
  }
  compactTree(root);
  return root;
}

// 只有一个子目录、没有文件的目录合并显示，例如 src/main/java
function compactTree(node: DirNode) {
  for (const [key, dir] of node.dirs) {
    let d = dir;
    let child = d.files.length === 0 && d.dirs.size === 1 ? d.dirs.values().next().value : undefined;
    while (child) {
      d = { name: `${d.name}/${child.name}`, path: child.path, dirs: child.dirs, files: child.files };
      child = d.files.length === 0 && d.dirs.size === 1 ? d.dirs.values().next().value : undefined;
    }
    node.dirs.set(key, d);
    compactTree(d);
  }
}

export const byName = (a: string, b: string) => a.localeCompare(b, 'en', { sensitivity: 'base' });
