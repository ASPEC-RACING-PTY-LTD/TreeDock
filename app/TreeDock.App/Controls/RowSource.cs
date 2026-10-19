// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Virtualised data sources for the folder tree and the details list. The
// engine owns the row models; these classes only materialise the rows a
// ListView realises, fetching them a page at a time (MASTER_PLAN §3.5, §6.6).

using System.Collections;
using System.Collections.Specialized;
using System.ComponentModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;
using TreeDock.Engine;
using TreeDock.Util;

namespace TreeDock.Controls;

/// <summary>What the folder tree shows next to each name.</summary>
public enum TreeValue { Size, Allocated, Files, Percent }

/// <summary>Global display preferences shared by all views.</summary>
internal static class Display
{
    public static TreeValue TreeValue { get; set; } = TreeValue.Allocated;
}

public sealed partial class RowItem
{
    internal RowItem(int index, in TdRow r, string name, bool tree)
    {
        Index = index;
        Node = r.node;
        Depth = r.depth;
        Name = name;
        IsDir = r.is_dir != 0;
        var flags = (NodeFlags)r.node_flags;
        bool expanded = (r.row_flags & 1) != 0;
        HasChildren = (r.row_flags & 2) != 0;
        IsExpanded = expanded;
        Expander = IsDir && HasChildren ? (expanded ? "" : "") : "";
        Icon = flags.HasFlag(NodeFlags.LinkNotFollowed) ? ""
            : flags.HasFlag(NodeFlags.Error) ? ""
            : flags.HasFlag(NodeFlags.Cloud) ? ""
            : IsDir ? "" : "";
        IconBrush = (Brush)Application.Current.Resources[
            flags.HasFlag(NodeFlags.Error) ? "SystemFillColorCautionBrush"
            : IsDir ? "TreeDockFolderBrush" : "TextFillColorSecondaryBrush"];
        Indent = new Thickness(tree ? r.depth * 16 : 0, 0, 0, 0);
        string prefix = IsDir && r.exact == 0 ? "≥ " : "";
        SizeText = prefix + Format.Size(r.logical);
        AllocatedText = prefix + Format.Size(r.allocated);
        double frac = r.parent_allocated > 0 ? (double)r.allocated / r.parent_allocated : (r.depth == 0 && tree ? 1 : 0);
        Fraction = Math.Clamp(frac, 0, 1);
        PercentText = Format.Percent(Fraction, always: true);
        FilesText = IsDir ? Format.Count(r.files) : "1";
        FoldersText = IsDir ? Format.Count(r.dirs) : "0";
        ModifiedText = Format.Date(r.mtime);
        TreeBarWidth = Fraction * 76;
        DetailsBarWidth = Fraction * 96;
        TreeValueText = Display.TreeValue switch
        {
            TreeValue.Size => SizeText,
            TreeValue.Files => IsDir ? Format.Count(r.files) : "",
            TreeValue.Percent => r.depth == 0 && tree ? "100 %" : PercentText,
            _ => AllocatedText,
        };
        var notes = new List<string>();
        if (flags.HasFlag(NodeFlags.LinkNotFollowed)) notes.Add("link, not followed");
        if (flags.HasFlag(NodeFlags.Cloud)) notes.Add("cloud placeholder");
        if (flags.HasFlag(NodeFlags.HardLink)) notes.Add("hard link");
        if (flags.HasFlag(NodeFlags.Error)) notes.Add("could not be read");
        if (flags.HasFlag(NodeFlags.Compressed)) notes.Add("compressed");
        Notes = string.Join(", ", notes);
        Level = r.depth + 1;
        AutomationName = $"{name}, {(IsDir ? "folder" : "file")}, {AllocatedText} on disk, {PercentText} of parent"
            + (IsDir ? $", {FilesText} files" : "")
            + (HasChildren && tree ? (expanded ? ", expanded" : ", collapsed") : "") + (Notes.Length > 0 ? $", {Notes}" : "");
        IsBold = tree && r.depth == 0;
    }

    public int Index { get; }
    public uint Node { get; }
    public int Depth { get; }
    public string Name { get; }
    public bool IsDir { get; }
    public bool HasChildren { get; }
    public bool IsExpanded { get; }
    public string Expander { get; }
    public string Icon { get; }
    public Brush IconBrush { get; }
    public Thickness Indent { get; }
    public string SizeText { get; }
    public string AllocatedText { get; }
    public double Fraction { get; }
    public string PercentText { get; }
    public string FilesText { get; }
    public string FoldersText { get; }
    public string ModifiedText { get; }
    public string TreeValueText { get; }
    public double TreeBarWidth { get; }
    public double DetailsBarWidth { get; }
    public string Notes { get; }
    public string AutomationName { get; }
    public int Level { get; }
    public bool IsBold { get; }
    public Windows.UI.Text.FontWeight Weight => IsBold ? Microsoft.UI.Text.FontWeights.SemiBold : Microsoft.UI.Text.FontWeights.Normal;
    public override string ToString() => AutomationName;
}

/// <summary>Fills <paramref name="buffer"/> with rows starting at <paramref name="first"/>; returns the count.</summary>
internal delegate int RowFetcher(uint first, TdRow[] buffer);

public sealed partial class RowSource : IList, INotifyCollectionChanged, INotifyPropertyChanged
{
    private const int Page = 128;
    private readonly Dictionary<int, RowItem> _cache = new();
    private readonly TdRow[] _buffer = new TdRow[Page];
    private readonly bool _tree;
    private Session? _session;
    private RowFetcher? _fetch;
    private int _count;

    public RowSource(bool tree) => _tree = tree;

    public event NotifyCollectionChangedEventHandler? CollectionChanged;
    public event PropertyChangedEventHandler? PropertyChanged;

    internal void Attach(Session? session, RowFetcher? fetch, int count)
    {
        _session = session;
        _fetch = fetch;
        Reset(count);
    }

    /// <summary>Discards cached rows and notifies the list (after expand/collapse/sort/refresh).</summary>
    public void Reset(int count)
    {
        _cache.Clear();
        _count = _session is null ? 0 : count;
        CollectionChanged?.Invoke(this, new NotifyCollectionChangedEventArgs(NotifyCollectionChangedAction.Reset));
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Count)));
    }

    public RowItem? Get(int index)
    {
        if (_session is null || _fetch is null || index < 0 || index >= _count) return null;
        if (_cache.TryGetValue(index, out var item)) return item;
        if (_cache.Count > 4096) _cache.Clear();
        int first = index / Page * Page;
        int n = _fetch((uint)first, _buffer);
        for (int i = 0; i < n; i++)
        {
            ref readonly var r = ref _buffer[i];
            _cache[first + i] = new RowItem(first + i, r, _session.Name(r.node), _tree);
        }
        return _cache.TryGetValue(index, out item) ? item : null;
    }

    public int Count => _count;
    public object? this[int index] { get => Get(index); set => throw new NotSupportedException(); }
    public bool IsFixedSize => false;
    public bool IsReadOnly => true;
    public bool IsSynchronized => false;
    public object SyncRoot => this;
    public int IndexOf(object? value) => value is RowItem r && r.Index < _count && _cache.TryGetValue(r.Index, out var c) && ReferenceEquals(c, r) ? r.Index : -1;
    public bool Contains(object? value) => IndexOf(value) >= 0;
    public int Add(object? value) => throw new NotSupportedException();
    public void Clear() => throw new NotSupportedException();
    public void Insert(int index, object? value) => throw new NotSupportedException();
    public void Remove(object? value) => throw new NotSupportedException();
    public void RemoveAt(int index) => throw new NotSupportedException();
    public void CopyTo(Array array, int index)
    {
        for (int i = 0; i < _count; i++) array.SetValue(Get(i), index + i);
    }

    public IEnumerator GetEnumerator()
    {
        for (int i = 0; i < _count; i++) yield return Get(i);
    }
}
