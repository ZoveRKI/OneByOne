import { useEffect, useEffectEvent, useState } from 'react';

/** 把文件拖进窗口时显示遮罩，松开后把第一个文件交给 onFile */
export function useFileDrop(onFile: (file: File) => void): boolean {
  const [active, setActive] = useState(false);
  const handleFile = useEffectEvent(onFile);

  useEffect(() => {
    // dragenter / dragleave 会在子元素之间反复触发，用计数判断是否真的离开了窗口
    let depth = 0;
    const hasFiles = (e: DragEvent) => Boolean(e.dataTransfer?.types.includes('Files'));
    const onEnter = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      depth += 1;
      setActive(true);
    };
    const onOver = (e: DragEvent) => {
      if (!hasFiles(e) || !e.dataTransfer) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = 'copy';
    };
    const onLeave = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      depth = Math.max(0, depth - 1);
      if (!depth) setActive(false);
    };
    const onDrop = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      depth = 0;
      setActive(false);
      const file = e.dataTransfer?.files[0];
      if (file) handleFile(file);
    };
    window.addEventListener('dragenter', onEnter);
    window.addEventListener('dragover', onOver);
    window.addEventListener('dragleave', onLeave);
    window.addEventListener('drop', onDrop);
    return () => {
      window.removeEventListener('dragenter', onEnter);
      window.removeEventListener('dragover', onOver);
      window.removeEventListener('dragleave', onLeave);
      window.removeEventListener('drop', onDrop);
    };
  }, []);

  return active;
}
