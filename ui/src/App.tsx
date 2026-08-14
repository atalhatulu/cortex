import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import { listen } from '@tauri-apps/api/event';
import { message } from '@tauri-apps/plugin-dialog';
import './index.css';

type FileItem = {
  name: string;
  size: number;
  type: string;
  modified_ts: number;
  path: string;
  is_dir: boolean;
};

type ProgressPayload = {
  processed: number;
  total: number;
  is_compressing: boolean;
};

function formatBytes(bytes: number) {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}

function formatDate(ts: number) {
  if (ts === 0) return '';
  const date = new Date(ts * 1000);
  return date.toLocaleString();
}

function App() {
  const [currentPath, setCurrentPath] = useState<string>('');
  const [files, setFiles] = useState<FileItem[]>([]);
  // Use a Set for multi-selection
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  
  const [statusMessage, setStatusMessage] = useState<string>('Ready.');
  const [isProcessing, setIsProcessing] = useState(false);
  const [password, setPassword] = useState<string>('');

  const [splitSize, setSplitSize] = useState<number>(0);
  // Compression mode: balanced (CTXT, default) | ratio (CTX8) | fast (CTXF)
  const [mode, setMode] = useState<string>('balanced');
  
  const [progress, setProgress] = useState<ProgressPayload | null>(null);
  const [speed, setSpeed] = useState<string>('');
  const [timeRemaining, setTimeRemaining] = useState<string>('');
  const lastProgressRef = useRef<{time: number, bytes: number} | null>(null);

  const loadDirectory = async (path: string = '') => {
    try {
      setStatusMessage('Loading...');
      const result: FileItem[] = await invoke('list_directory', { path });
      setFiles(result);
      if (result.length > 0) {
        const parentPath = await invoke<string>('get_parent_directory', { path: result[0].path });
        if (!path) setCurrentPath(parentPath);
        else setCurrentPath(path);
      } else if (path) {
        setCurrentPath(path);
      }
      setSelectedPaths(new Set());
      setStatusMessage('Ready.');
    } catch (err: any) {
      console.error(err);
      setStatusMessage(`Error loading directory: ${err}`);
    }
  };

  useEffect(() => {
    async function checkCliArgs() {
      try {
        const args: string[] = await invoke('get_cli_args');
        const action = args.length >= 2 ? args[1] : '';
        const firstTarget = args.length >= 3 ? args[2] : '';

        if (args.length >= 3 && action === "compress") {
          // `%F` can expand to multiple files — bundle them all into one archive.
          const inputPaths = args.slice(2).filter((p) => !p.startsWith('-'));
          const first = inputPaths[0];

          setIsProcessing(true);
          setStatusMessage('Compressing with CORTEX...');
          setProgress({ processed: 0, total: 100, is_compressing: true });

          const parent = await invoke<string>('get_parent_directory', { path: first });
          const defaultName = first.split(/[\\/]/).pop() || "archive";
          const defaultOut = `${parent}/${defaultName}.ctx`;

          try {
            const res = await invoke<string>('compress_cmd', {
                inputPaths,
                outputPath: defaultOut,
                password: null,
                level: 3,
                splitSize: 0,
                mode
            });
            await message(res, { title: 'CORTEX', kind: 'info' });
            await invoke('exit_app');
          } catch (e: any) {
              setStatusMessage(`Error: ${e}`);
              await message(`Error: ${e}`, { title: 'Compression Failed', kind: 'error' });
              setIsProcessing(false);
          }
          return;
        }

        if (args.length >= 3 && action === "extract") {
          const inputPath = firstTarget;
          setIsProcessing(true);
          setStatusMessage('Extracting with CORTEX...');
          setProgress({ processed: 0, total: 100, is_compressing: false });

          const defaultOutDir = inputPath.replace(/\.ctx(\.\d{3})?$/, '_restored');
          try {
            const res = await invoke<string>('decompress_cmd', {
                inputPath,
                outputPath: defaultOutDir,
                password: null
            });
            await message(res, { title: 'CORTEX', kind: 'info' });
            await invoke('exit_app');
          } catch (e: any) {
              setStatusMessage(`Error: ${e}`);
              await message(`Error: ${e}`, { title: 'Decompression Failed', kind: 'error' });
              setIsProcessing(false);
          }
          return;
        }

        // `open` from a ServiceMenu / Nautilus script, or a bare path from
        // double-clicking an archive, both mean "show this location": a .ctx
        // opens as a virtual folder via list_directory. Split volumes
        // (archive.ctx.001) are normalized to the base file so the lookup
        // succeeds.
        const openTarget = action === "open"
          ? firstTarget
          : (args.length === 2 && action && !['compress', 'extract', 'open'].includes(action) && !action.startsWith('-')
              ? action
              : '');
        if (openTarget) {
          const normalized = openTarget.replace(/\.ctx\.\d{3}$/i, '.ctx');
          await loadDirectory(normalized);
          return;
        }
      } catch (e) {
        console.error(e);
      }
      loadDirectory();
    }
    checkCliArgs();

    const unlistenDrop = listen('tauri://drop', (event: any) => {
      const paths = event.payload as string[];
      if (paths && paths.length > 0) {
         invoke<string>('get_parent_directory', { path: paths[0] }).then(parent => {
            loadDirectory(parent);
         });
      }
    });

    const unlistenProgress = listen<ProgressPayload>('progress', (event) => {
      const p = event.payload;
      setProgress(p);
      
      const now = Date.now();
      if (!lastProgressRef.current) {
         lastProgressRef.current = { time: now, bytes: p.processed };
      } else {
         const timeDiff = (now - lastProgressRef.current.time) / 1000;
         if (timeDiff >= 0.2) { 
           const bytesDiff = p.processed - lastProgressRef.current.bytes;
           const bytesPerSec = bytesDiff / timeDiff;
           setSpeed(`${formatBytes(bytesPerSec)}/s`);
           
           const remainingBytes = p.total - p.processed;
           const remainingSeconds = bytesPerSec > 0 ? remainingBytes / bytesPerSec : 0;
           
           if (remainingSeconds > 0 && remainingSeconds < 3600) {
             setTimeRemaining(`${Math.ceil(remainingSeconds)}s remaining`);
           } else {
             setTimeRemaining('Calculating...');
           }
               
           lastProgressRef.current = { time: now, bytes: p.processed };
         }
      }
    });

    return () => {
      unlistenDrop.then(f => f());
      unlistenProgress.then(f => f());
    };
  }, []);

  const goUp = async () => {
    if (!currentPath) return;
    try {
      const parent = await invoke<string>('get_parent_directory', { path: currentPath });
      if (parent && parent !== currentPath) {
        loadDirectory(parent);
      }
    } catch (e) {
      console.error(e);
    }
  };

  const handleAddFile = async () => {
    try {
      const selected = await open({ multiple: true, directory: false });
      if (selected !== null && Array.isArray(selected) && selected.length > 0) {
        const parent = await invoke<string>('get_parent_directory', { path: selected[0] });
        loadDirectory(parent);
      } else if (typeof selected === 'string') {
        const parent = await invoke<string>('get_parent_directory', { path: selected });
        loadDirectory(parent);
      }
    } catch (err) {
      console.error(err);
      setStatusMessage('Error selecting file.');
    }
  };

  const toggleSelection = (path: string, multi: boolean) => {
    setSelectedPaths(prev => {
      const newSet = multi ? new Set(prev) : new Set<string>();
      if (newSet.has(path)) {
        newSet.delete(path);
      } else {
        newSet.add(path);
      }
      return newSet;
    });
  };

  const handleCompress = async () => {
    if (selectedPaths.size === 0) return;
    setIsProcessing(true);
    setStatusMessage('Compressing...');
    
    // Estimate total size for progress bar initialization
    const totalSize = files.filter(f => selectedPaths.has(f.path)).reduce((acc, f) => acc + f.size, 0) || 100;
    setProgress({ processed: 0, total: totalSize, is_compressing: true });
    lastProgressRef.current = null;
    setSpeed('Calculating...');
    setTimeRemaining('');
    
    try {
      // Create a default archive name based on the first selected item or current folder
      let defaultName = 'archive';
      if (selectedPaths.size === 1) {
        const firstFile = files.find(f => f.path === Array.from(selectedPaths)[0]);
        if (firstFile) defaultName = firstFile.name;
      }
      const defaultOut = `${currentPath}/${defaultName}.ctx`;
      
      const outPath = await save({ defaultPath: defaultOut });
      if (outPath) {
        const res = await invoke<string>('compress_cmd', { 
            inputPaths: Array.from(selectedPaths), 
            outputPath: outPath,
            password: password ? password : null,
            level: 3,
            splitSize: splitSize > 0 ? splitSize * 1024 * 1024 : 0,
            mode
        });
        setStatusMessage(res);
        loadDirectory(currentPath);
        await message(res, { title: 'CORTEX Compression Complete', kind: 'info' });
      } else {
        setStatusMessage('Operation cancelled.');
      }
    } catch (err: any) {
      setStatusMessage(`Error: ${err}`);
      await message(`Error: ${err}`, { title: 'Compression Failed', kind: 'error' });
    } finally {
      setIsProcessing(false);
      setProgress(null);
    }
  };

  const handleExtract = async () => {
    const selectedArray = Array.from(selectedPaths);
    if (selectedArray.length !== 1) {
      setStatusMessage('Please select a single .ctx file to extract.');
      return;
    }
    const archivePath = selectedArray[0];
    if (!archivePath.endsWith('.ctx') && !archivePath.endsWith('.ctx.001')) return;

    setIsProcessing(true);
    setStatusMessage('Extracting...');
    setProgress({ processed: 0, total: 100, is_compressing: false }); // size unknown initially
    lastProgressRef.current = null;
    setSpeed('Calculating...');
    setTimeRemaining('');

    try {
      const defaultOutDir = archivePath.replace(/\.ctx(\.\d{3})?$/, '_restored');
      const outPath = await save({ defaultPath: defaultOutDir });
      if (outPath) {
        const res = await invoke<string>('decompress_cmd', { 
            inputPath: archivePath, 
            outputPath: outPath,
            password: password ? password : null
        });
        setStatusMessage(res);
        loadDirectory(currentPath);
        await message(res, { title: 'CORTEX Decompression Complete', kind: 'info' });
      } else {
        setStatusMessage('Operation cancelled.');
      }
    } catch (err: any) {
      setStatusMessage(`Error: ${err}`);
      await message(`Error: ${err}`, { title: 'Decompression Failed', kind: 'error' });
    } finally {
      setIsProcessing(false);
      setProgress(null);
    }
  };

  const percent = progress && progress.total > 0 
    ? Math.min(100, Math.round((progress.processed / progress.total) * 100)) 
    : 0;

  const isExtractDisabled = selectedPaths.size !== 1 || (!Array.from(selectedPaths)[0].endsWith('.ctx') && !Array.from(selectedPaths)[0].endsWith('.001'));

  // Pure display helper: breaks the current path into clickable segments.
  // Clicking a segment just calls the existing loadDirectory — no new logic.
  const pathSegments = currentPath.split(/([\\/])/).filter(Boolean);
  const isErrorStatus = statusMessage.toLowerCase().startsWith('error');

  return (
    <div className="cortex-app">


      {/* TOOLBAR */}
      <div className="toolbar">
        <button className="tool-btn" onClick={handleAddFile} disabled={isProcessing}>
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><line x1="12" y1="5" x2="12" y2="19"></line><line x1="5" y1="12" x2="19" y2="12"></line></svg>
          Add
        </button>
        <button className="tool-btn tool-btn--primary" onClick={handleCompress} disabled={selectedPaths.size === 0 || isProcessing}>
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path><polyline points="17 8 12 3 7 8"></polyline><line x1="12" y1="3" x2="12" y2="15"></line></svg>
          Compress
        </button>
        <button className="tool-btn" onClick={handleExtract} disabled={isExtractDisabled || isProcessing}>
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path><polyline points="7 10 12 15 17 10"></polyline><line x1="12" y1="15" x2="12" y2="3"></line></svg>
          Extract
        </button>
        <div className="separator"></div>
        <button className="tool-btn" onClick={() => loadDirectory(currentPath)} disabled={isProcessing}>
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><polyline points="23 4 23 10 17 10"></polyline><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"></path></svg>
          Refresh
        </button>
        
        <div style={{ flex: 1 }}></div>
        
        <div className="brand-status" style={{ border: 'none', background: 'transparent', paddingRight: '16px' }}>
          <span className={`dot ${isProcessing ? 'busy' : ''}`}></span>
          {isProcessing ? 'Working' : 'Ready'}
        </div>

        <div className="settings-group">
          <select
            className="level-select"
            value={mode}
            onChange={(e) => setMode(e.target.value)}
            disabled={isProcessing}
            title="Compression mode: Balanced (default) / Max ratio / Max speed"
          >
            <option value="balanced">Balanced (CTXT)</option>
            <option value="ratio">Max ratio (CTX8)</option>
            <option value="fast">Max speed (CTXF)</option>
          </select>
          <select 
            className="level-select"
            value={splitSize}
            onChange={(e) => setSplitSize(Number(e.target.value))}
            disabled={isProcessing}
          >
            <option value={0}>Don't split</option>
            <option value={10}>Split to 10 MB</option>
            <option value={100}>Split to 100 MB</option>
            <option value={1024}>Split to 1 GB</option>
          </select>


          <input 
            type="password" 
            placeholder="Set Password (Optional)" 
            className="password-input"
            value={password}
            onChange={e => setPassword(e.target.value)}
            disabled={isProcessing}
          />
        </div>
      </div>

      {/* ADDRESS BAR */}
      <div className="address-bar">
        <button className="up-btn" onClick={goUp} disabled={isProcessing}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><polyline points="15 18 9 12 15 6"></polyline></svg>
        </button>
        <div className="path-input">
          {pathSegments.map((seg, i) => {
            const isSep = seg === '/' || seg === '\\';
            if (isSep) return <span key={i} className="crumb-sep">{seg}</span>;
            const segPath = pathSegments.slice(0, i + 1).join('');
            const isLast = i === pathSegments.length - 1;
            return (
              <span
                key={i}
                className="crumb"
                onClick={() => { if (!isLast && !isProcessing) loadDirectory(segPath); }}
              >
                {seg}
              </span>
            );
          })}
        </div>
      </div>

      {/* FILE EXPLORER MAIN CONTENT */}
      <div className="explorer-content">
        <table className="file-table">
          <thead>
            <tr>
              <th>Name</th>
              <th>Size</th>
              <th>Type</th>
              <th>Modified</th>
            </tr>
          </thead>
          <tbody>
            {files.map((f, i) => (
              <tr 
                key={i} 
                className={selectedPaths.has(f.path) ? 'selected' : ''}
                onClick={(e) => {
                  if (isProcessing) return;
                  toggleSelection(f.path, e.ctrlKey || e.metaKey);
                }}
                onDoubleClick={() => {
                  if ((f.is_dir || f.name.endsWith('.ctx')) && !isProcessing) {
                    loadDirectory(f.path);
                  }
                }}
              >
                <td>
                  <div className="file-name-cell">
                    {f.is_dir ? (
                      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>
                    ) : f.name.endsWith('.ctx') ? (
                      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--accent-color)" strokeWidth="2"><path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"></path><polyline points="3.27 6.96 12 12.01 20.73 6.96"></polyline><line x1="12" y1="22.08" x2="12" y2="12"></line></svg>
                    ) : (
                      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"></path><polyline points="13 2 13 9 20 9"></polyline></svg>
                    )}
                    {f.name}
                  </div>
                </td>
                <td>{formatBytes(f.size)}</td>
                <td>{f.type}</td>
                <td>{formatDate(f.modified_ts)}</td>
              </tr>
            ))}
            {files.length === 0 && (
              <tr>
                <td colSpan={4}>
                  <div className="empty-state">
                    <svg className="empty-icon" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" style={{ margin: '0 auto' }}><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>
                    <div className="empty-title">Folder is empty or permission denied</div>
                    <div className="empty-hint">Drop files here or use Add to bring items into view</div>
                  </div>
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      {/* OVERLAY PROGRESS */}
      {progress && (
        <div className="progress-overlay">
          <div className="progress-card">
            <h3>{progress.is_compressing ? 'Compressing with CORTEX' : 'Decompressing'}</h3>
            {progress.processed === 0 ? (
              <div className="progress-info" style={{ justifyContent: 'center' }}>
                <span>Analyzing & Chunking (Parallel Processing)...</span>
              </div>
            ) : (
              <div className="progress-info">
                <span>{formatBytes(progress.processed)} / {formatBytes(progress.total)}</span>
                <span>{percent}%</span>
              </div>
            )}
            <div className="progress-track">
              <div className="progress-fill" style={{ width: `${percent}%` }}></div>
            </div>
            <div className="progress-stats">
              <span>Speed: {progress.processed === 0 ? 'Calculating...' : speed}</span>
              <span>{progress.processed === 0 ? 'Please wait' : timeRemaining}</span>
            </div>
          </div>
        </div>
      )}

      {/* STATUS BAR */}
      <div className="status-bar">
        <div className="status-text">
          <span className={`dot ${isProcessing ? 'busy' : isErrorStatus ? 'error' : ''}`}></span>
          {statusMessage}
        </div>
        <div className="status-details">
          {selectedPaths.size > 0 ? `${selectedPaths.size} items selected` : `${files.length} items`}
        </div>
      </div>
    </div>
  );
}

export default App;
