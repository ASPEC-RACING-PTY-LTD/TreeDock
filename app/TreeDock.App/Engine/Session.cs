// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Safe wrapper over the generated P/Invoke bindings (Native.g.cs).
// All engine calls made from the UI thread are O(page) (MASTER_PLAN §3.5).

namespace TreeDock.Engine;

internal unsafe delegate uint StrFn(ushort* buf, uint cap);

internal static unsafe class Interop
{
    public static string Str(StrFn f)
    {
        Span<char> stack = stackalloc char[256];
        fixed (char* p = stack)
        {
            uint n = f((ushort*)p, (uint)stack.Length);
            if (n <= stack.Length) return new string(p, 0, (int)n);
            var arr = new char[n];
            fixed (char* q = arr)
            {
                f((ushort*)q, n);
                return new string(q, 0, (int)n);
            }
        }
    }

    public static string LastError() => Str((b, c) => Native.td_last_error(b, c));

    public static EngineException Error(string what) => new($"{what}: {LastError()}");
}

internal sealed class EngineException(string message) : Exception(message);

internal enum ScanState : uint { Scanning = 1, Done = 2, Cancelled = 3, Failed = 4 }

internal enum SortColumn : uint { Name = 0, Size = 1, Allocated = 2, Files = 3, Modified = 4, Extension = 5 }

[Flags]
internal enum NodeFlags : ushort
{
    Dir = 1 << 0, Reparse = 1 << 1, LinkNotFollowed = 1 << 2, HardLink = 1 << 3, Compressed = 1 << 4,
    Sparse = 1 << 5, Cloud = 1 << 6, Error = 1 << 7, Pending = 1 << 8, Estimated = 1 << 9, Deleted = 1 << 10, Offline = 1 << 11,
}

internal sealed record VolumeEntry(string Root, string Label, string FileSystem, ulong Total, ulong Free, uint DriveType, uint Media)
{
    public string MediaName => Media switch { 1 => "SSD", 2 => "HDD", 3 => "Removable", 4 => "Network", _ => DriveType == 4 ? "Network" : "Drive" };
    public string Title => string.IsNullOrEmpty(Label) ? $"{MediaName} ({Root.TrimEnd('\\')})" : $"{Label} ({Root.TrimEnd('\\')})";
    public double UsedFraction => Total == 0 ? 0 : (double)(Total - Free) / Total;
}

internal sealed record OpOutcome(int Status, string Path, string Message);

internal sealed unsafe class Session : IDisposable
{
    public ulong Handle { get; private set; }

    private Session(ulong h) => Handle = h;

    public static Session Scan(string path, uint mode = 0, uint hardlinks = 0)
    {
        var opts = new TdScanOptions { mode = mode, hardlinks = hardlinks };
        fixed (char* p = path)
        {
            ulong h = Native.td_scan_start((ushort*)p, (uint)path.Length, &opts);
            if (h == 0) throw Interop.Error("Cannot scan");
            return new Session(h);
        }
    }

    public static Session OpenSnapshot(string path)
    {
        fixed (char* p = path)
        {
            ulong h = Native.td_snapshot_open((ushort*)p, (uint)path.Length);
            if (h == 0) throw Interop.Error("Cannot open snapshot");
            return new Session(h);
        }
    }

    public static bool IsElevated => Native.td_is_elevated() != 0;

    public static List<VolumeEntry> Volumes()
    {
        uint n = Native.td_volumes(null, 0);
        var arr = new TdVolume[n];
        var list = new List<VolumeEntry>();
        fixed (TdVolume* p = arr)
        {
            n = Math.Min(n, Native.td_volumes(p, n));
            for (int i = 0; i < n; i++)
            {
                TdVolume* v = p + i;
                list.Add(new VolumeEntry(Fixed(v->root, 8), Fixed(v->label, 64), Fixed(v->filesystem, 16), v->total, v->free, v->drive_type, v->media));
            }
        }
        return list;
    }

    private static string Fixed(ushort* p, int max)
    {
        int len = 0;
        while (len < max && p[len] != 0) len++;
        return new string((char*)p, 0, len);
    }

    public void Dispose()
    {
        if (Handle != 0) Native.td_session_close(Handle);
        Handle = 0;
    }

    public TdProgress Progress()
    {
        TdProgress p;
        Native.td_progress(Handle, &p);
        return p;
    }

    public ScanState State => (ScanState)Progress().state;
    public string CurrentPath => Interop.Str((b, c) => Native.td_current_path(Handle, b, c));
    public void Cancel() => Native.td_cancel(Handle);
    public void Pause(bool paused) => Native.td_pause(Handle, paused ? 1u : 0u);

    public TdSummary Summary()
    {
        TdSummary s;
        if (Native.td_summary(Handle, &s) != 0) throw Interop.Error("Summary");
        return s;
    }

    public string RootPath => Meta(0);
    public string ScanMode => Meta(1);
    public string FileSystem => Meta(2);
    public string VolumeLabel => Meta(3);
    public string[] Notes => Meta(4).Split('\n', StringSplitOptions.RemoveEmptyEntries);
    private string Meta(uint which) => Interop.Str((b, c) => Native.td_meta_string(Handle, which, b, c));

    public List<(string Path, string Message)> Errors()
    {
        uint n = Native.td_error_count(Handle);
        var list = new List<(string, string)>((int)n);
        for (uint i = 0; i < n; i++)
        {
            var s = Interop.Str((b, c) => Native.td_error_at(Handle, i, b, c));
            int t = s.IndexOf('\t');
            list.Add(t < 0 ? (s, "") : (s[..t], s[(t + 1)..]));
        }
        return list;
    }

    // ---- nodes

    public TdNode Node(uint node)
    {
        TdNode n;
        if (Native.td_node(Handle, node, &n) != 0) throw Interop.Error("Node");
        return n;
    }

    public string Name(uint node) => Interop.Str((b, c) => Native.td_node_name(Handle, node, b, c));
    public string PathOf(uint node) => Interop.Str((b, c) => Native.td_node_path(Handle, node, b, c));
    public string ExtName(ushort ext) => Interop.Str((b, c) => Native.td_ext_name(Handle, ext, b, c));

    // ---- tree-list view

    public uint ViewReset(uint root, SortColumn sort, bool descending) => Native.td_view_reset(Handle, root, (uint)sort, descending ? 1u : 0u);
    public uint ViewLength => Native.td_view_len(Handle);
    public uint ViewToggle(uint row) => Native.td_view_toggle(Handle, row);
    public uint ViewRefresh() => Native.td_view_refresh(Handle);
    public uint ViewSort(SortColumn sort, bool descending) => Native.td_view_sort(Handle, (uint)sort, descending ? 1u : 0u);
    public long ViewReveal(uint node) => Native.td_view_reveal(Handle, node);

    public int ViewRows(uint first, Span<TdRow> dest)
    {
        fixed (TdRow* p = dest) return (int)Native.td_view_rows(Handle, first, (uint)dest.Length, p);
    }

    // ---- details list (children of one folder)

    public uint ListReset(uint node, SortColumn sort, bool descending) => Native.td_list_reset(Handle, node, (uint)sort, descending ? 1u : 0u);

    public int ListRows(uint first, Span<TdRow> dest)
    {
        fixed (TdRow* p = dest) return (int)Native.td_list_rows(Handle, first, (uint)dest.Length, p);
    }

    public long ListIndexOf(uint node) => Native.td_list_index_of(Handle, node);

    public int Sunburst(uint root, uint maxDepth, float minSweep, SunburstArc[] dest)
    {
        fixed (SunburstArc* p = dest) return (int)Native.td_sunburst(Handle, root, maxDepth, minSweep, 0, p, (uint)dest.Length);
    }

    /// <summary>True/false when the current user does or does not have full control of <paramref name="path"/>; null if unknown.</summary>
    public static bool? HasFullAccess(string path)
    {
        fixed (char* p = path)
        {
            int r = Native.td_has_full_access((ushort*)p, (uint)path.Length);
            return r < 0 ? null : r == 1;
        }
    }

    // ---- queries

    public uint[] TopFiles(uint root, int n)
    {
        var arr = new uint[n];
        fixed (uint* p = arr)
        {
            uint got = Native.td_top_files(Handle, root, (uint)n, 0, p);
            return arr[..(int)got];
        }
    }

    public TdExtStat[] ExtStats(uint root, int cap)
    {
        var arr = new TdExtStat[cap];
        fixed (TdExtStat* p = arr)
        {
            uint total = Native.td_ext_stats(Handle, root, p, (uint)cap);
            return arr[..(int)Math.Min(total, (uint)cap)];
        }
    }

    public int Treemap(uint root, TdTreemapOptions opts, TreemapRect[] dest)
    {
        fixed (TreemapRect* p = dest) return (int)Native.td_treemap(Handle, root, &opts, p, (uint)dest.Length);
    }

    // ---- persistence and export

    public void SaveSnapshot(string path)
    {
        fixed (char* p = path)
            if (Native.td_snapshot_save(Handle, (ushort*)p, (uint)path.Length) != 0) throw Interop.Error("Save failed");
    }

    public void Export(string path, bool json, int maxDepth = -1, bool dirsOnly = false)
    {
        fixed (char* p = path)
            if (Native.td_export(Handle, (ushort*)p, (uint)path.Length, json ? 1u : 0u, maxDepth, dirsOnly ? 1u : 0u) != 0)
                throw Interop.Error("Export failed");
    }

    // ---- file operations

    public void RecycleStart(uint[] nodes, nint hwnd)
    {
        fixed (uint* p = nodes)
            if (Native.td_recycle_start(Handle, p, (uint)nodes.Length, hwnd) != 0) throw Interop.Error("Cannot recycle");
    }

    public TdOpProgress OpProgress()
    {
        TdOpProgress p;
        Native.td_op_progress(Handle, &p);
        return p;
    }

    public void OpCancel() => Native.td_op_cancel(Handle);

    public List<OpOutcome> OpResults(uint count)
    {
        var list = new List<OpOutcome>();
        for (uint i = 0; i < count; i++)
        {
            var parts = Interop.Str((b, c) => Native.td_op_result(Handle, i, b, c)).Split('\t', 3);
            if (parts.Length == 3 && int.TryParse(parts[0], out int st)) list.Add(new OpOutcome(st, parts[1], parts[2]));
        }
        return list;
    }

    public static string? ProtectedReason(string path)
    {
        fixed (char* p = path)
        {
            ushort* pp = (ushort*)p;
            int len = path.Length;
            var r = Interop.Str((b, c) => Native.td_protected_reason(pp, (uint)len, b, c));
            return r.Length == 0 ? null : r;
        }
    }
}
