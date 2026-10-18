/*
 * OneByOne 播放器：把一次代码改动按步骤播放，每一步显示对应的 diff 和讲解。
 */
import { useCallback, useEffect, useLayoutEffect, useReducer, useRef, useState } from 'react';
import { DropOverlay, HelpDialog, Landing } from './components/Overlays';
import { Sidebar } from './components/Sidebar';
import { DiffView, Explain, StepHead } from './components/StepView';
import { TopBar } from './components/TopBar';
import type { Model } from './core/model';
import type { Bridge } from './data/mcp';
import { loadInitial, parseWalkthrough, type Initial } from './data/source';
import { useFileDrop } from './hooks/useFileDrop';
import { useHostView } from './hooks/useHostView';
import { useAutoplay, useHashSync } from './hooks/usePlayerEffects';
import { useTheme } from './hooks/useTheme';
import { errorMessage } from './lib/format';
import { initialState, readHash, reducer, trackOf } from './state/player';

const TOAST_MS = 6000;

interface Props {
  load?: (bridge: Bridge | null) => Promise<Initial>;
  /** 页面嵌在 iframe 里时和 MCP 客户端的连接，见 data/mcp.ts */
  bridge?: Bridge | null;
}

export function App({ load = loadInitial, bridge = null }: Props) {
  const [state, dispatch] = useReducer(reducer, initialState);
  const [landing, setLanding] = useState<{ message: string; error: boolean } | null>(null);
  const [toast, setToast] = useState<{ text: string; id: number } | null>(null);
  // 握手成功后才有：在 MCP 客户端的对话里显示
  const host = useHostView(bridge);
  const { pref: themePref, cycle: cycleTheme } = useTheme(host?.theme);
  const fileInput = useRef<HTMLInputElement>(null);
  const help = useRef<HTMLDialogElement>(null);
  const stage = useRef<HTMLElement>(null);
  const diff = useRef<HTMLDivElement>(null);
  // 点击文件跳到别的步骤后，等新步骤渲染出来再滚动到这个文件
  const pendingScroll = useRef<string | null>(null);
  const modelRef = useRef<Model | null>(null);
  const { model } = state;

  useEffect(() => {
    modelRef.current = model;
    if (model) document.title = `${model.title} · OneByOne`;
  }, [model]);

  // 已经有数据时只弹提示，不打断当前的浏览
  const showError = useCallback((message: string) => {
    if (modelRef.current) setToast({ text: message, id: Date.now() });
    else setLanding({ message, error: true });
  }, []);

  const loadText = useCallback(
    (text: string, name: string, restore: boolean) => {
      const result = parseWalkthrough(text, name);
      if ('error' in result) {
        showError(result.error);
        return;
      }
      dispatch({ type: 'load', model: result.model, target: restore ? readHash(location.hash) : null });
      setLanding(null);
    },
    [showError],
  );

  const readFile = useCallback(
    async (file: File) => {
      try {
        loadText(await file.text(), file.name, false);
      } catch (err) {
        showError(`读取 ${file.name} 失败：${errorMessage(err)}`);
      }
    },
    [loadText, showError],
  );

  useEffect(() => {
    let cancelled = false;
    void load(bridge).then((initial) => {
      if (cancelled) return;
      if (initial.kind === 'data') loadText(initial.text, initial.name, initial.restore);
      else if (initial.kind === 'landing') setLanding({ message: initial.message, error: false });
      // 在 MCP 客户端里：数据随工具结果送来，还没送到时先显示等待提示
      else if (!modelRef.current)
        setLanding((current) => current ?? { message: '正在等待讲解数据……', error: false });
    });
    return () => {
      cancelled = true;
    };
  }, [load, bridge, loadText]);

  // 工具结果随时可能送来，握手超时以后也一样：讲解数据直接加载，出错时显示工具返回的说明
  useEffect(
    () =>
      bridge?.onResult((initial) => {
        if (initial.kind === 'data') loadText(initial.text, initial.name, false);
        else if (initial.kind === 'landing') showError(initial.message);
      }),
    [bridge, loadText, showError],
  );

  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(null), TOAST_MS);
    return () => window.clearTimeout(timer);
  }, [toast]);

  useAutoplay(state, dispatch);
  useHashSync(state, dispatch);
  const dragging = useFileDrop((file) => void readFile(file));

  const pickFile = useCallback(() => fileInput.current?.click(), []);
  const openHelp = useCallback(() => {
    const dialog = help.current;
    if (dialog && !dialog.open) dialog.showModal();
  }, []);

  const scrollToFile = useCallback((path: string) => {
    const sections = diff.current?.querySelectorAll<HTMLElement>('.diff-file') ?? [];
    const node = [...sections].find((s) => s.dataset.diffFile === path);
    if (!node || typeof node.scrollIntoView !== 'function') return;
    const reduce =
      typeof window.matchMedia === 'function' &&
      window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    node.scrollIntoView({ block: 'start', behavior: reduce ? 'auto' : 'smooth' });
  }, []);

  // 换步骤后：要么滚动到刚点的文件，要么回到顶部（窄屏时整个步骤区一起滚动）
  const stepKey = `${state.serial}-${state.mode}-${state.index}`;
  const lastStepKey = useRef('');
  useLayoutEffect(() => {
    if (lastStepKey.current === stepKey) return;
    lastStepKey.current = stepKey;
    const target = pendingScroll.current;
    pendingScroll.current = null;
    if (target) scrollToFile(target);
    else if (stage.current) stage.current.scrollTop = 0;
  }, [stepKey, scrollToFile]);

  // 点击文件：当前步骤就涉及它则滚动到对应位置，否则跳到下一个涉及它的步骤
  const openFile = (path: string) => {
    if (!model) return;
    const steps = trackOf(model, state.mode).fileSteps.get(path) ?? [];
    if (!steps.length) return;
    if (steps.includes(state.index)) {
      scrollToFile(path);
      return;
    }
    pendingScroll.current = path;
    dispatch({ type: 'go', index: steps.find((k) => k > state.index) ?? steps[0] ?? 0 });
  };

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.metaKey || e.altKey || e.isComposing || help.current?.open) return;
      const t = e.target instanceof HTMLElement ? e.target : null;
      if (t?.classList.contains('scrubber-input')) {
        // 焦点在进度条上时，方向键、Home、End 交给滑块自己处理，其余快捷键照常
        if (/^(Arrow|Home$|End$|Page)/.test(e.key)) return;
      } else if (t && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName))) {
        return;
      }
      if (t && (e.key === 'Enter' || e.key === ' ') && t.getAttribute('role') === 'button') {
        e.preventDefault();
        t.click();
        return;
      }
      const onControl = Boolean(t && /^(BUTTON|SUMMARY|A)$/.test(t.tagName));
      switch (e.key) {
        case 'ArrowRight':
          dispatch({ type: 'step', delta: 1 });
          break;
        case 'ArrowLeft':
          dispatch({ type: 'step', delta: -1 });
          break;
        case 'Home':
          dispatch({ type: 'first' });
          break;
        case 'End':
          dispatch({ type: 'last' });
          break;
        case ' ':
          if (onControl) return;
          dispatch({ type: 'togglePlay' });
          break;
        case 'm':
        case 'M':
          dispatch({ type: 'toggleMode' });
          break;
        case 'w':
        case 'W':
          dispatch({ type: 'toggleWrap' });
          break;
        case 'o':
        case 'O':
          pickFile();
          break;
        case 't':
        case 'T':
          cycleTheme();
          break;
        case '?':
          openHelp();
          break;
        default:
          return;
      }
      e.preventDefault();
    };
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [cycleTheme, openHelp, pickFile]);

  const track = model ? trackOf(model, state.mode) : null;
  const step = track?.items[state.index];
  const goTo = (index: number) => dispatch({ type: 'go', index });
  // 在对话里内嵌显示（不是全屏）时用紧凑布局：不显示文件树，工具栏只占一行，其余高度都给步骤内容
  const compact = host !== null && host.displayMode !== 'fullscreen';

  return (
    <>
      {model && track && step ? (
        <div className={compact ? 'app compact' : 'app'}>
          <TopBar
            state={state}
            model={model}
            track={track}
            dispatch={dispatch}
            themePref={themePref}
            onCycleTheme={cycleTheme}
            onOpenFile={host ? undefined : pickFile}
            onHelp={openHelp}
            fullscreen={
              bridge && host?.canFullscreen
                ? { active: host.displayMode === 'fullscreen', onToggle: bridge.toggleFullscreen }
                : undefined
            }
            compact={compact}
          />
          <main className="main">
            {!compact && (
              <Sidebar
                model={model}
                track={track}
                index={state.index}
                collapsed={state.collapsed}
                onGo={goTo}
                onOpenFile={openFile}
                onToggleDir={(path) => dispatch({ type: 'toggleDir', path })}
                onGotoUncovered={() => dispatch({ type: 'gotoUncovered' })}
              />
            )}
            <section ref={stage} className="stage">
              <div className="step-head">
                <StepHead key={stepKey} step={step} index={state.index} onJump={scrollToFile} />
                <button
                  className={`text-btn${state.wrap ? ' on' : ''}`}
                  type="button"
                  aria-pressed={state.wrap}
                  title="代码自动换行（W）"
                  onClick={() => dispatch({ type: 'toggleWrap' })}
                >
                  自动换行
                </button>
              </div>
              <div className="stage-body">
                <DiffView
                  key={`diff-${stepKey}`}
                  ref={diff}
                  step={step}
                  files={model.files}
                  wrap={state.wrap}
                />
                <Explain key={`explain-${stepKey}`} step={step} items={track.items} onGo={goTo} />
              </div>
            </section>
          </main>
        </div>
      ) : (
        landing && (
          <Landing
            message={landing.message}
            error={landing.error}
            onOpen={pickFile}
            embedded={host !== null}
          />
        )
      )}
      <input
        ref={fileInput}
        type="file"
        accept=".json,application/json"
        hidden
        onChange={(e) => {
          const file = e.target.files?.[0];
          e.target.value = '';
          if (file) void readFile(file);
        }}
      />
      {dragging && <DropOverlay />}
      {toast && (
        <div className="toast" role="status">
          {toast.text}
        </div>
      )}
      <HelpDialog ref={help} />
    </>
  );
}
