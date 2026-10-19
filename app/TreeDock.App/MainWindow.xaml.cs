// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
using System.Diagnostics;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.Windows.Storage.Pickers;
using TreeDock.Controls;
using TreeDock.Engine;
using TreeDock.Util;
using Windows.ApplicationModel.DataTransfer;
using VirtualKey = Windows.System.VirtualKey;

namespace TreeDock;

public sealed partial class DriveRow
{
    // Kept private: x:Bind data types must not expose internal types (the XAML compiler fails on them).
    private readonly VolumeEntry Volume;
    internal DriveRow(VolumeEntry v) => Volume = v;
    public string Root => Volume.Root;
    public string Title => string.IsNullOrEmpty(Volume.Label) ? Volume.Root : $"{Volume.Root}  {Volume.Label}";
    public string Glyph => Volume.DriveType switch { 4 => "\uE8CE", 2 => "\uE88E", 5 => "\uE958", _ => "\uEDA2" };
    public string TotalText => Format.Size(Volume.Total);
    public string FreeSpaceText => Format.Size(Volume.Free);
    private double FreeFraction => Volume.Total == 0 ? 0 : (double)Volume.Free / Volume.Total;
    public string FreePercentText => Format.Percent(FreeFraction, always: true);
    public double FreeBarWidth => FreeFraction * 94;
    public Brush FreeBrush => new SolidColorBrush(FreeFraction < 0.1 ? Windows.UI.Color.FromArgb(200, 214, 72, 60) : Windows.UI.Color.FromArgb(200, 76, 175, 80));
    public override string ToString() => $"{Title}, {FreeSpaceText} free of {TotalText}, {FreePercentText} free";
}

public sealed partial class ExtRow
{
    public string Name { get; init; } = "";
    public string Category { get; init; } = "";
    public string SizeText { get; init; } = "";
    public string FilesText { get; init; } = "";
    public string PercentText { get; init; } = "";
    public double BarWidth { get; init; }
    public Brush? Swatch { get; init; }
    public override string ToString() => $"{Name}, {Category}, {SizeText}, {FilesText} files, {PercentText}";
}

public sealed partial class FileRow
{
    public uint Node { get; init; }
    public string Name { get; init; } = "";
    public string Folder { get; init; } = "";
    public string SizeText { get; init; } = "";
    public override string ToString() => $"{Name}, {SizeText}, in {Folder}";
}

public sealed partial class BarRow
{
    public uint Node { get; init; }
    public string Name { get; init; } = "";
    public string Icon { get; init; } = "";
    public Brush? IconBrush { get; init; }
    public Brush? BarBrush { get; init; }
    public double BarWidth { get; init; }
    public string SizeText { get; init; } = "";
    public override string ToString() => $"{Name}, {SizeText}";
}

public sealed partial class MainWindow : Window
{
    private Session? _session;
    private string? _scanPath;
    private readonly RowSource _treeRows = new(tree: true);
    private readonly RowSource _detailRows = new(tree: false);
    private readonly DispatcherQueueTimer _timer;
    private SortColumn _sort = SortColumn.Allocated;
    private bool _descending = true;
    private uint _selectedNode;
    private uint _folder;
    private bool _suppressSelection;
    private DateTime _lastRefresh = DateTime.MinValue;
    private bool _opRunning;
    private ScrollViewer? _treeScroll;
    private readonly Stack<uint> _back = new();
    private readonly Stack<uint> _forward = new();
    private List<VolumeEntry> _volumes = [];
    private readonly List<string> _recent = [];

    public MainWindow(string? initialPath)
    {
        InitializeComponent();
        AppWindow.SetIcon("Assets\\TreeDock.ico");
        double scale = GetDpiForWindow(WinRT.Interop.WindowNative.GetWindowHandle(this)) / 96.0;
        AppWindow.Resize(new Windows.Graphics.SizeInt32((int)(1480 * scale), (int)(920 * scale)));
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(TitleBar);
        TreeList.ItemsSource = _treeRows;
        DetailsList.ItemsSource = _detailRows;
        _timer = DispatcherQueue.CreateTimer();
        _timer.Interval = TimeSpan.FromMilliseconds(250);
        _timer.Tick += (_, _) => Tick();

        Treemap.Hovered += ShowHover;
        Sunburst.Hovered += ShowHover;
        Treemap.NodeClicked += n => SelectNode(n, chartClick: true);
        Sunburst.NodeClicked += n => SelectNode(n, chartClick: true);
        Treemap.NodeActivated += ActivateNode;
        Sunburst.NodeActivated += ActivateNode;

        bool elevated = Session.IsElevated;
        ElevateButton.Visibility = elevated ? Visibility.Collapsed : Visibility.Visible;
        Title = elevated ? "TreeDock (Administrator)" : "TreeDock";
        TitleText.Text = Title + (App.TestMode ? "  [test mode]" : "");
        LoadVolumes();
        ShowSummary(null);
        if (!string.IsNullOrWhiteSpace(initialPath))
        {
            PathBox.Text = initialPath;
            // A path passed on the command line (e.g. after elevation) was already approved: scan directly.
            DispatcherQueue.TryEnqueue(() => StartScan(initialPath));
        }
        Closed += (_, _) => { _timer.Stop(); _session?.Dispose(); };
    }

    [System.Runtime.InteropServices.LibraryImport("user32.dll")]
    private static partial uint GetDpiForWindow(nint hwnd);

    private nint Hwnd => WinRT.Interop.WindowNative.GetWindowHandle(this);

    // ------------------------------------------------------------------ drives and path bar

    private void LoadVolumes()
    {
        try
        {
            _volumes = Session.Volumes();
            DrivesList.ItemsSource = _volumes.Select(v => new DriveRow(v)).ToList();
            UpdatePathItems(null);
        }
        catch (Exception e)
        {
            ShowInfo(InfoBarSeverity.Error, "Could not list drives", e.Message);
        }
    }

    private void PathBox_DropDownOpened(object? sender, object e) { }

    /// <summary>Dropdown = recently scanned paths followed by drive roots; selects <paramref name="current"/>.</summary>
    private void UpdatePathItems(string? current)
    {
        if (current is not null)
        {
            _recent.RemoveAll(p => string.Equals(p, current, StringComparison.OrdinalIgnoreCase));
            _recent.Insert(0, current);
            if (_recent.Count > 10) _recent.RemoveAt(_recent.Count - 1);
        }
        var items = _recent.Concat(_volumes.Select(v => v.Root)).Distinct(StringComparer.OrdinalIgnoreCase).ToList();
        PathBox.ItemsSource = items;
        if (current is not null) PathBox.SelectedItem = items.First(i => string.Equals(i, current, StringComparison.OrdinalIgnoreCase));
    }

    private DateTime _lastSubmit = DateTime.MinValue;

    private void PathBox_TextSubmitted(ComboBox sender, ComboBoxTextSubmittedEventArgs args)
    {
        // Raised by Enter, and also when focus leaves the box with edited text (e.g. clicking Scan).
        args.Handled = true;
        _lastSubmit = DateTime.UtcNow;
        RequestScan(args.Text);
    }

    private void ScanButton_Click(object sender, RoutedEventArgs e)
    {
        if (DateTime.UtcNow - _lastSubmit < TimeSpan.FromMilliseconds(750)) return; // already submitted by focus change
        // ComboBox.Text lags behind the editable text box when the typed text matches a list item.
        string? typed = FindChild<TextBox>(PathBox)?.Text;
        if (App.TestMode) Log.Write($"Scan click: typed='{typed}' text='{PathBox.Text}' selected='{PathBox.SelectedItem}'");
        RequestScan(!string.IsNullOrWhiteSpace(typed) ? typed : PathBox.SelectedItem as string ?? PathBox.Text);
    }


    private void FocusPath_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        PathBox.Focus(FocusState.Keyboard);
        args.Handled = true;
    }

    private void Drive_ItemClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is DriveRow d)
        {
            PathBox.Text = d.Root;
            RequestScan(d.Root);
        }
    }

    private async void Browse_Click(object sender, RoutedEventArgs e)
    {
        try
        {
            var result = await new FolderPicker(AppWindow.Id).PickSingleFolderAsync();
            if (result is null) return;
            PathBox.Text = result.Path;
            RequestScan(result.Path);
        }
        catch (Exception ex)
        {
            ShowInfo(InfoBarSeverity.Error, "Could not open the folder picker", ex.Message);
        }
    }

    // ------------------------------------------------------------------ elevation-aware scan request

    /// <summary>
    /// Validates the target, then asks for administrator rights (UAC) when scanning a whole local drive or a
    /// folder the current user does not have full control of; otherwise scans directly.
    /// </summary>
    private (string Path, DateTime At) _lastRequest = ("", DateTime.MinValue);

    private void RequestScan(string? input)
    {
        if (_opRunning) return;
        string path = (input ?? "").Trim().Trim('"');
        if (App.TestMode) Log.Write($"RequestScan '{path}'");
        // Enter, focus changes and the Scan button can all submit the same text: act once.
        if (string.Equals(path, _lastRequest.Path, StringComparison.OrdinalIgnoreCase) && DateTime.UtcNow - _lastRequest.At < TimeSpan.FromSeconds(1)) return;
        _lastRequest = (path, DateTime.UtcNow);
        if (path.Length == 0)
        {
            ShowInfo(InfoBarSeverity.Informational, "Choose what to scan", "Type a drive (C:\\) or folder path, pick a drive from the list, or browse for a folder.");
            return;
        }
        if (path.Length == 2 && path[1] == ':') path += "\\";
        if (!Directory.Exists(path))
        {
            ShowInfo(InfoBarSeverity.Error, "Folder not found", $"\"{path}\" does not exist or is not a folder.");
            return;
        }
        if (!Session.IsElevated)
        {
            bool driveRoot = IsLocalDriveRoot(path);
            bool? full = driveRoot ? null : Session.HasFullAccess(path);
            if (driveRoot || full == false)
            {
                string why = driveRoot
                    ? "Scanning a whole drive works best with administrator rights (fast NTFS scan, protected folders included)."
                    : "You do not have full access to this folder, so some contents may be unreadable and file management (delete, move) may fail.";
                if (TryElevate(path, why)) return;
                ShowInfo(InfoBarSeverity.Warning, "Scanning without administrator rights", why + " Protected folders will be listed as unreadable.");
                StartScan(path, keepInfo: true);
                return;
            }
        }
        StartScan(path);
    }

    private bool IsLocalDriveRoot(string path)
    {
        string full = Path.GetFullPath(path);
        return full.Length == 3 && full[1] == ':' &&
            _volumes.Any(v => string.Equals(v.Root, full, StringComparison.OrdinalIgnoreCase) && v.DriveType is 2 or 3);
    }

    /// <summary>Relaunches elevated with <paramref name="path"/>; returns true if the elevated instance started.</summary>
    private bool TryElevate(string? path, string reason)
    {
        if (App.TestMode)
        {
            // Test mode never shows a UAC prompt; it records that elevation would have been requested.
            Log.Write($"Elevation requested (simulated in test mode) for {path}: {reason}");
            StatusText.Text = "Elevation requested (simulated in test mode).";
            return false;
        }
        try
        {
            var psi = new ProcessStartInfo(Environment.ProcessPath!) { UseShellExecute = true, Verb = "runas" };
            if (path is not null) psi.ArgumentList.Add(path);
            Process.Start(psi);
            Close();
            return true;
        }
        catch (System.ComponentModel.Win32Exception)
        {
            return false; // UAC prompt declined
        }
    }

    private void Elevate_Click(object sender, RoutedEventArgs e) =>
        TryElevate(_scanPath, "Restart requested from the toolbar.");

    // ------------------------------------------------------------------ scanning

    private void StartScan(string path, bool keepInfo = false)
    {
        if (_opRunning) return;
        try
        {
            var s = Session.Scan(path);
            AttachSession(s);
            _scanPath = path;
            UpdatePathItems(path);
            if (!keepInfo) Info.IsOpen = false;
            SetScanning(true);
            _timer.Start();
        }
        catch (Exception e)
        {
            ShowInfo(InfoBarSeverity.Error, "Cannot scan", e.Message);
        }
    }

    private void AttachSession(Session s)
    {
        _timer.Stop();
        _session?.Dispose();
        _session = s;
        _selectedNode = 0;
        _folder = 0;
        _back.Clear();
        _forward.Clear();
        uint n = s.ViewReset(0, SortColumn.Allocated, true);
        _treeRows.Attach(s, (first, buf) => s.ViewRows(first, buf), (int)n);
        _detailRows.Attach(s, (first, buf) => s.ListRows(first, buf), 0);
        TreePlaceholder.Visibility = Visibility.Collapsed;
        ExportButton.IsEnabled = true;
        UpdateSortHeaders();
        UpdateNavButtons();
        UpdatePanes();
    }

    private void SetScanning(bool scanning)
    {
        StopButton.IsEnabled = scanning;
        PauseButton.IsEnabled = scanning;
        PauseButton.IsChecked = false;
        PauseButton.Label = "Pause";
        RefreshButton.IsEnabled = !scanning && _scanPath is not null;
        Busy.IsActive = scanning;
    }

    private void Tick()
    {
        if (_session is null) return;
        if (_opRunning) { PollOperation(); return; }
        var p = _session.Progress();
        var state = (ScanState)p.state;
        if (state == ScanState.Scanning)
        {
            StatusText.Text = $"{(p.paused != 0 ? "Paused" : "Scanning")} · {Format.Count(p.files)} files · {Format.Count(p.dirs)} folders · {Format.Size(p.allocated)} · {Format.Duration(p.elapsed_ms)}   {_session.CurrentPath}";
            if (DateTime.UtcNow - _lastRefresh > TimeSpan.FromSeconds(1.5))
            {
                _lastRefresh = DateTime.UtcNow;
                RefreshTree();
                UpdatePanes();
            }
            return;
        }
        _timer.Stop();
        SetScanning(false);
        RefreshTree();
        UpdatePanes();
        ShowScanResult(state);
    }

    private void ShowScanResult(ScanState state)
    {
        if (_session is null) return;
        var sum = _session.Summary();
        var p = _session.Progress();
        FreeText.Text = sum.volume_total > 0
            ? $"Free space: {Format.Size(sum.volume_free)} (of {Format.Size(sum.volume_total)})"
            : "";
        StatusText.Text = $"{Format.Count(sum.files)} files · {Format.Count(sum.dirs)} folders · " +
            (sum.cluster_size > 0 ? $"{Format.Count(sum.cluster_size)} bytes per cluster ({_session.FileSystem}) · " : "") +
            $"{_session.ScanMode} · {Format.Duration(p.elapsed_ms)}";
        var errors = _session.Errors();
        var notes = _session.Notes.ToList();
        if (sum.cloud_files > 0)
            notes.Add($"{Format.Count(sum.cloud_files)} cloud files ({Format.Size(sum.cloud_logical)}) are not stored on this PC and use little or no local space.");
        if (sum.links_not_followed > 0)
            notes.Add($"{Format.Count(sum.links_not_followed)} junctions or symbolic links were not followed, so their targets are not counted twice.");
        if (sum.volume_total > 0 && _session.RootPath.Length == 3)
        {
            ulong used = sum.volume_total - sum.volume_free;
            long gap = (long)used - (long)sum.allocated;
            if (Math.Abs(gap) > (long)(used / 100))
                notes.Add(gap > 0
                    ? $"{Format.Size((ulong)gap)} of used space was not found in files: system metadata, restore points or folders that could not be read."
                    : $"Files account for {Format.Size((ulong)-gap)} more than the drive's used space, usually because hard links are counted at each location.");
        }
        if (state == ScanState.Cancelled && errors.Count > 0 && sum.files == 0)
        {
            ShowInfo(InfoBarSeverity.Error, "Scan failed", errors[0].Message);
            return;
        }
        string title = state switch
        {
            ScanState.Cancelled => "Scan stopped — results are incomplete",
            ScanState.Failed => "Scan failed",
            _ => errors.Count > 0 ? $"{errors.Count} folders could not be read" : "Scan complete",
        };
        if (errors.Count > 0)
            notes.Insert(0, (Session.IsElevated ? "" : "Run as administrator to read them. ") + "Totals for affected folders are minimums (≥). Examples: "
                + string.Join("; ", errors.Take(3).Select(e => e.Path)));
        if (Info.IsOpen && Info.Severity == InfoBarSeverity.Warning && Info.Title.StartsWith("Scanning without", StringComparison.Ordinal))
            notes.Insert(0, Info.Message);
        var sev = state != ScanState.Done || errors.Count > 0 ? InfoBarSeverity.Warning : InfoBarSeverity.Success;
        ShowInfo(sev, title, string.Join("\n", notes));
    }

    private void Stop_Click(object sender, RoutedEventArgs e) => _session?.Cancel();

    private void Pause_Click(object sender, RoutedEventArgs e)
    {
        if (_session is null) return;
        bool paused = PauseButton.IsChecked == true;
        _session.Pause(paused);
        PauseButton.Label = paused ? "Resume" : "Pause";
    }

    private void Refresh_Click(object sender, RoutedEventArgs e)
    {
        if (_scanPath is not null) StartScan(_scanPath);
    }

    // ------------------------------------------------------------------ display modes

    private void Mode_Click(object sender, RoutedEventArgs e)
    {
        if (sender is not AppBarToggleButton b || !Enum.TryParse<TreeValue>(b.Tag as string, out var mode)) return;
        Display.TreeValue = mode;
        foreach (var t in new[] { ModeSize, ModeAllocated, ModeFiles, ModePercent }) t.IsChecked = t == b;
        RefreshTree();
    }

    private void Unit_Click(object sender, RoutedEventArgs e)
    {
        if (sender is not RadioMenuFlyoutItem i || !Enum.TryParse<SizeUnit>(i.Tag as string, out var unit)) return;
        Format.Unit = unit;
        UnitButton.Label = $"Unit: {(unit == SizeUnit.Auto ? "Auto" : unit.ToString())}";
        LoadVolumes();
        RefreshTree();
        UpdatePanes();
    }

    // ------------------------------------------------------------------ folder tree

    private ScrollViewer? TreeScroll => _treeScroll ??= FindChild<ScrollViewer>(TreeList);

    private static T? FindChild<T>(DependencyObject root) where T : DependencyObject
    {
        for (int i = 0; i < VisualTreeHelper.GetChildrenCount(root); i++)
        {
            var c = VisualTreeHelper.GetChild(root, i);
            if (c is T t) return t;
            if (FindChild<T>(c) is T r) return r;
        }
        return null;
    }

    /// <summary>Re-reads tree rows from the engine, keeping scroll position and selection.</summary>
    private void RefreshTree(uint? count = null)
    {
        if (_session is null) return;
        double offset = TreeScroll?.VerticalOffset ?? 0;
        uint n = count ?? _session.ViewRefresh();
        _suppressSelection = true;
        _treeRows.Reset((int)n);
        long idx = _session.ViewReveal(_selectedNode);
        if (idx >= 0) TreeList.SelectedIndex = (int)idx;
        _suppressSelection = false;
        if (offset > 0) DispatcherQueue.TryEnqueue(() => TreeScroll?.ChangeView(null, offset, null, true));
    }

    private void List_ContainerContentChanging(ListViewBase sender, ContainerContentChangingEventArgs args)
    {
        if (args.Item is RowItem r)
        {
            AutomationProperties.SetName(args.ItemContainer, r.AutomationName);
            if (sender == TreeList) AutomationProperties.SetLevel(args.ItemContainer, r.Level);
        }
    }

    private void TreeList_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_suppressSelection || TreeList.SelectedItem is not RowItem r || _session is null) return;
        _selectedNode = r.Node;
        Navigate(r.IsDir ? r.Node : _session.Node(r.Node).parent, record: true);
    }

    private void Toggle(RowItem r)
    {
        if (_session is null || !r.IsDir) return;
        _selectedNode = r.Node;
        RefreshTree(_session.ViewToggle((uint)r.Index));
    }

    private void TreeList_Tapped(object sender, TappedRoutedEventArgs e)
    {
        // A tap on the chevron toggles without needing a double-click.
        if (e.OriginalSource is TextBlock { Text: "\uE76C" or "\uE70D" } tb && tb.DataContext is RowItem r)
        {
            Toggle(r);
            e.Handled = true;
        }
    }

    private void TreeList_DoubleTapped(object sender, DoubleTappedRoutedEventArgs e)
    {
        if (TreeList.SelectedItem is not RowItem r) return;
        if (r.IsDir) Toggle(r); else Shell.Reveal(PathOf(r.Node));
    }

    private void TreeList_KeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (_session is null || TreeList.SelectedItem is not RowItem r) return;
        switch (e.Key)
        {
            case VirtualKey.Right when r.IsDir && r.HasChildren && !r.IsExpanded:
            case VirtualKey.Left when r.IsDir && r.IsExpanded:
            case VirtualKey.Space:
                Toggle(r);
                e.Handled = true;
                break;
            case VirtualKey.Left when r.Depth > 0:
                SelectNode(_session.Node(r.Node).parent, chartClick: false);
                e.Handled = true;
                break;
            case VirtualKey.Enter:
                if (r.IsDir) Toggle(r); else Shell.Reveal(PathOf(r.Node));
                e.Handled = true;
                break;
            default:
                e.Handled = HandleCommonKeys(e.Key, SelectedNodes(TreeList));
                break;
        }
    }

    private bool HandleCommonKeys(VirtualKey key, uint[] nodes)
    {
        if (nodes.Length == 0) return false;
        bool ctrl = InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Control).HasFlag(Windows.UI.Core.CoreVirtualKeyStates.Down);
        bool alt = InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Menu).HasFlag(Windows.UI.Core.CoreVirtualKeyStates.Down);
        switch (key)
        {
            case VirtualKey.Delete: _ = Recycle(nodes); return true;
            case VirtualKey.C when ctrl: CopyText(nodes.Select(PathOf)); return true;
            case VirtualKey.E when ctrl: Shell.Reveal(PathOf(nodes[0])); return true;
            case VirtualKey.Enter when alt: Shell.ShowProperties(Hwnd, PathOf(nodes[0])); return true;
            default: return false;
        }
    }

    private static uint[] SelectedNodes(ListViewBase list) => list.SelectedItems.Select(o => o switch
    {
        RowItem r => r.Node,
        FileRow f => f.Node,
        BarRow b => b.Node,
        _ => uint.MaxValue,
    }).Where(n => n != uint.MaxValue).Distinct().ToArray();

    private string PathOf(uint node) => _session?.PathOf(node) ?? "";

    // ------------------------------------------------------------------ navigation

    /// <summary>Shows <paramref name="folder"/> in the right-hand panes.</summary>
    private void Navigate(uint folder, bool record)
    {
        if (_session is null) return;
        if (record && folder != _folder)
        {
            _back.Push(_folder);
            _forward.Clear();
        }
        _folder = folder;
        UpdateNavButtons();
        UpdatePanes();
    }

    private void UpdateNavButtons()
    {
        BackButton.IsEnabled = _back.Count > 0;
        ForwardButton.IsEnabled = _forward.Count > 0;
        UpButton.IsEnabled = _session is not null && _folder != 0;
    }

    private void Back_Click(object sender, RoutedEventArgs e)
    {
        if (_back.Count == 0) return;
        _forward.Push(_folder);
        GoTo(_back.Pop());
    }

    private void Forward_Click(object sender, RoutedEventArgs e)
    {
        if (_forward.Count == 0) return;
        _back.Push(_folder);
        GoTo(_forward.Pop());
    }

    private void Up_Click(object sender, RoutedEventArgs e)
    {
        if (_session is null || _folder == 0) return;
        SelectNode(_session.Node(_folder).parent, chartClick: false);
    }

    private void GoTo(uint folder)
    {
        _selectedNode = folder;
        SelectInTree(folder);
        Navigate(folder, record: false);
    }

    private void SelectInTree(uint node)
    {
        if (_session is null) return;
        long idx = _session.ViewReveal(node);
        RefreshTree(_session.ViewLength);
        if (idx >= 0)
        {
            _suppressSelection = true;
            TreeList.SelectedIndex = (int)idx;
            _suppressSelection = false;
            TreeList.ScrollIntoView(_treeRows.Get((int)idx));
        }
    }

    /// <summary>Selects a node everywhere. Chart clicks only highlight; other sources also open its folder.</summary>
    private void SelectNode(uint node, bool chartClick)
    {
        if (_session is null) return;
        _selectedNode = node;
        SelectInTree(node);
        if (chartClick)
        {
            Treemap.SetSelected(node);
            Sunburst.SetSelected(node);
            ShowSummary(node);
            return;
        }
        var info = _session.Node(node);
        Navigate(info.is_dir != 0 ? node : info.parent, record: true);
    }

    /// <summary>Double-click in a chart: open folders, reveal files in File Explorer.</summary>
    private void ActivateNode(uint node)
    {
        if (_session is null) return;
        if (_session.Node(node).is_dir != 0) SelectNode(node, chartClick: false);
        else Shell.Reveal(PathOf(node));
    }

    // ------------------------------------------------------------------ right-hand panes

    private void Tabs_SelectionChanged(SelectorBar sender, SelectorBarSelectionChangedEventArgs args) => UpdatePanes();

    private void ChartKind_SelectionChanged(SelectorBar sender, SelectorBarSelectionChangedEventArgs args) => UpdatePanes();

    private void UpdatePanes()
    {
        var tab = Tabs.SelectedItem;
        ChartPane.Visibility = tab == TabChart ? Visibility.Visible : Visibility.Collapsed;
        DetailsPane.Visibility = tab == TabDetails ? Visibility.Visible : Visibility.Collapsed;
        TypesList.Visibility = tab == TabExtensions ? Visibility.Visible : Visibility.Collapsed;
        TopList.Visibility = tab == TabTop ? Visibility.Visible : Visibility.Collapsed;
        var kind = ChartKind.SelectedItem;
        Treemap.Visibility = kind == KindTreemap ? Visibility.Visible : Visibility.Collapsed;
        Sunburst.Visibility = kind == KindSunburst ? Visibility.Visible : Visibility.Collapsed;
        BarsList.Visibility = kind == KindBars ? Visibility.Visible : Visibility.Collapsed;
        ShowSummary(_folder);
        if (_session is null) return;

        var crumbs = new List<Crumb>();
        for (uint n = _folder; ; n = _session.Node(n).parent)
        {
            crumbs.Add(new Crumb(n, _session.Name(n)));
            if (n == 0) break;
        }
        crumbs.Reverse();
        Breadcrumb.ItemsSource = crumbs;

        if (tab == TabChart)
        {
            if (kind == KindSunburst) Sunburst.SetData(_session, _folder, _selectedNode);
            else if (kind == KindBars) LoadBars();
            else Treemap.SetData(_session, _folder, _selectedNode);
        }
        else if (tab == TabDetails) LoadDetails();
        else if (tab == TabExtensions) LoadTypes();
        else LoadTop();
    }

    private void ShowSummary(uint? node)
    {
        if (_session is null || node is null)
        {
            SumSize.Text = SumAllocated.Text = SumFiles.Text = SumFolders.Text = SumModified.Text = "—";
            return;
        }
        var n = _session.Node(node.Value);
        string ge = n.is_dir != 0 && n.exact == 0 ? "≥ " : "";
        SumSize.Text = ge + Format.Size(n.logical);
        SumAllocated.Text = ge + Format.Size(n.allocated);
        SumFiles.Text = n.is_dir != 0 ? Format.Count(n.files) : "1";
        SumFolders.Text = n.is_dir != 0 ? Format.Count(n.dirs) : "0";
        SumModified.Text = Format.Date(n.mtime);
    }

    private sealed record Crumb(uint Node, string Name)
    {
        public override string ToString() => Name.TrimEnd('\\');
    }

    private void Breadcrumb_ItemClicked(BreadcrumbBar sender, BreadcrumbBarItemClickedEventArgs args)
    {
        if (args.Item is Crumb c) SelectNode(c.Node, chartClick: false);
    }

    private void LoadDetails()
    {
        if (_session is null) return;
        uint n = _session.ListReset(_folder, _sort, _descending);
        _detailRows.Reset((int)n);
        long idx = _session.ListIndexOf(_selectedNode);
        if (idx >= 0)
        {
            DetailsList.SelectedIndex = (int)idx;
            DetailsList.ScrollIntoView(_detailRows.Get((int)idx));
        }
    }

    private void DetailsList_DoubleTapped(object sender, DoubleTappedRoutedEventArgs e)
    {
        if (DetailsList.SelectedItem is not RowItem r) return;
        if (r.IsDir) SelectNode(r.Node, chartClick: false); else Shell.Reveal(PathOf(r.Node));
    }

    private void DetailsList_KeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (DetailsList.SelectedItem is not RowItem r) return;
        if (e.Key == VirtualKey.Enter)
        {
            if (r.IsDir) SelectNode(r.Node, chartClick: false); else Shell.Reveal(PathOf(r.Node));
            e.Handled = true;
        }
        else if (e.Key == VirtualKey.Back)
        {
            Up_Click(this, new RoutedEventArgs());
            e.Handled = true;
        }
        else e.Handled = HandleCommonKeys(e.Key, SelectedNodes(DetailsList));
    }

    private void Sort_Click(object sender, RoutedEventArgs e)
    {
        if (sender is not Button b || !uint.TryParse(b.Tag as string, out uint col)) return;
        var c = (SortColumn)col;
        _descending = c == _sort ? !_descending : c != SortColumn.Name;
        _sort = c;
        UpdateSortHeaders();
        LoadDetails();
    }

    private void UpdateSortHeaders()
    {
        (Button b, string text, SortColumn c)[] cols =
        [
            (SortName, "Name", SortColumn.Name), (SortSize, "Size", SortColumn.Size), (SortAllocated, "Allocated", SortColumn.Allocated),
            (SortFiles, "Files", SortColumn.Files), (SortModified, "Last Modified", SortColumn.Modified),
        ];
        foreach (var (b, text, c) in cols)
        {
            b.Content = c == _sort ? $"{text} {(_descending ? "▼" : "▲")}" : text;
            AutomationProperties.SetName(b, c == _sort ? $"{text}, sorted {(_descending ? "descending" : "ascending")}" : $"Sort by {text}");
        }
    }

    private void LoadBars()
    {
        if (_session is null) return;
        int n = (int)Math.Min(_session.ListReset(_folder, SortColumn.Allocated, true), 40u);
        var buf = new TdRow[n];
        buf = buf[.._session.ListRows(0, buf)];
        ulong max = buf.Length > 0 ? Math.Max(1, buf.Max(r => r.allocated)) : 1;
        double width = Math.Max(80, BarsList.ActualWidth * 0.55 - 40);
        BarsList.ItemsSource = buf.Select(r =>
        {
            bool dir = r.is_dir != 0;
            string name = _session.Name(r.node);
            var color = dir ? Windows.UI.Color.FromArgb(255, 91, 141, 239) : FileCategories.ColorFor(_session.ExtName(r.ext_id));
            return new BarRow
            {
                Node = r.node,
                Name = name,
                Icon = dir ? "\uE8B7" : "\uE8A5",
                IconBrush = (Brush)Application.Current.Resources[dir ? "TreeDockFolderBrush" : "TextFillColorSecondaryBrush"],
                BarBrush = new SolidColorBrush(color),
                BarWidth = width * r.allocated / max,
                SizeText = Format.Size(r.allocated),
            };
        }).ToList();
    }

    private void BarsList_DoubleTapped(object sender, DoubleTappedRoutedEventArgs e)
    {
        if (BarsList.SelectedItem is BarRow b) ActivateNode(b.Node);
    }

    private void LoadTypes()
    {
        if (_session is null) return;
        var stats = _session.ExtStats(_folder, 300);
        double total = Math.Max(1, _session.Node(_folder).allocated);
        TypesList.ItemsSource = stats.Select(s =>
        {
            string ext = _session.ExtName(s.ext_id);
            double frac = s.allocated / total;
            return new ExtRow
            {
                Name = ext.Length == 0 ? "(no extension)" : "." + ext,
                Category = FileCategories.Of(ext)?.Name ?? "Other",
                SizeText = Format.Size(s.allocated),
                FilesText = Format.Count(s.files),
                PercentText = Format.Percent(frac, always: true),
                BarWidth = frac * 108,
                Swatch = new SolidColorBrush(FileCategories.ColorFor(ext)),
            };
        }).ToList();
    }

    private void LoadTop()
    {
        if (_session is null) return;
        TopList.ItemsSource = _session.TopFiles(_folder, 200).Select(n =>
        {
            string path = _session.PathOf(n);
            int slash = path.LastIndexOf('\\');
            return new FileRow
            {
                Node = n,
                Name = slash >= 0 ? path[(slash + 1)..] : path,
                Folder = slash >= 0 ? path[..slash] : "",
                SizeText = Format.Size(_session.Node(n).allocated),
            };
        }).ToList();
    }

    private void TopList_DoubleTapped(object sender, DoubleTappedRoutedEventArgs e)
    {
        if (TopList.SelectedItem is FileRow f) Shell.Reveal(PathOf(f.Node));
    }

    private void ShowHover(uint n)
    {
        if (n == uint.MaxValue || _session is null) { HoverText.Text = ""; return; }
        HoverText.Text = $"{_session.PathOf(n)} — {Format.Size(_session.Node(n).allocated)}";
    }

    // ------------------------------------------------------------------ context menu and actions

    private void List_RightTapped(object sender, RightTappedRoutedEventArgs e)
    {
        if (sender is not ListViewBase list || (e.OriginalSource as FrameworkElement)?.DataContext is not object item) return;
        if (item is not (RowItem or FileRow or BarRow)) return;
        if (!list.SelectedItems.Contains(item)) list.SelectedItem = item;
        var nodes = SelectedNodes(list);
        if (nodes.Length > 0) ItemMenu(nodes).ShowAt(list, e.GetPosition(list));
        e.Handled = true;
    }

    private MenuFlyout ItemMenu(uint[] nodes)
    {
        var menu = new MenuFlyout();
        if (_session is null || nodes.Length == 0) return menu;
        uint first = nodes[0];
        bool single = nodes.Length == 1;
        bool isDir = _session.Node(first).is_dir != 0;
        void Add(string text, string glyph, Action act, bool enabled = true, string? accel = null)
        {
            var i = new MenuFlyoutItem { Text = text, Icon = new FontIcon { Glyph = glyph }, IsEnabled = enabled };
            if (accel is not null) i.KeyboardAcceleratorTextOverride = accel;
            i.Click += (_, _) => act();
            menu.Items.Add(i);
        }
        Add("Open", "\uE8E5", () => Shell.Open(PathOf(first)), single);
        Add("Open file location", "\uEC50", () => Shell.Reveal(PathOf(first)), single, "Ctrl+E");
        Add("Properties", "\uE946", () => Shell.ShowProperties(Hwnd, PathOf(first)), single, "Alt+Enter");
        menu.Items.Add(new MenuFlyoutSeparator());
        Add(single ? "Copy path" : $"Copy {nodes.Length} paths", "\uE8C8", () => CopyText(nodes.Select(PathOf)), accel: "Ctrl+C");
        Add(single ? "Copy name" : $"Copy {nodes.Length} names", "\uE8C8", () => CopyText(nodes.Select(n => _session.Name(n))));
        if (single && isDir)
        {
            Add("Scan this folder", "\uE721", () => RequestScan(PathOf(first)));
            Add("Show in chart", "\uEB05", () => SelectNode(first, chartClick: false));
        }
        menu.Items.Add(new MenuFlyoutSeparator());
        bool scanning = (ScanState)_session.Progress().state == ScanState.Scanning;
        Add(single ? "Move to Recycle Bin…" : $"Move {nodes.Length} items to Recycle Bin…", "\uE74D", () => _ = Recycle(nodes), !scanning, "Del");
        return menu;
    }

    private void CopyText(IEnumerable<string> lines)
    {
        var list = lines.ToList();
        var dp = new DataPackage();
        dp.SetText(string.Join(Environment.NewLine, list));
        Clipboard.SetContent(dp);
        StatusText.Text = list.Count == 1 ? "Copied to the clipboard." : $"{list.Count} lines copied to the clipboard.";
    }

    private async Task Recycle(uint[] nodes)
    {
        if (_session is null || _opRunning) return;
        nodes = nodes.Where(n => n != 0).ToArray();
        if (nodes.Length == 0) return;
        if ((ScanState)_session.Progress().state == ScanState.Scanning)
        {
            ShowInfo(InfoBarSeverity.Informational, "Wait for the scan to finish", "Files can be moved to the Recycle Bin once the scan has completed or been stopped.");
            return;
        }
        var paths = nodes.Select(PathOf).ToList();
        var blocked = paths.Select(p => (p, r: Session.ProtectedReason(p))).Where(x => x.r is not null).ToList();
        var noAccess = Session.IsElevated ? [] : paths.Where(p => Session.HasFullAccess(p) == false).ToList();
        ulong total = nodes.Aggregate(0UL, (a, n) => a + _session.Node(n).allocated);
        var body = new StackPanel { Spacing = 8 };
        body.Children.Add(new TextBlock
        {
            Text = $"{nodes.Length} item(s), {Format.Size(total)} on disk, will be moved to the Recycle Bin. You can restore them from there.",
            TextWrapping = TextWrapping.Wrap,
        });
        body.Children.Add(new TextBlock
        {
            Text = string.Join("\n", paths.Take(8)) + (paths.Count > 8 ? $"\n… and {paths.Count - 8} more" : ""),
            TextWrapping = TextWrapping.Wrap,
            FontFamily = new FontFamily("Consolas"),
            FontSize = 12,
        });
        if (blocked.Count > 0)
            body.Children.Add(new InfoBar { IsOpen = true, IsClosable = false, Severity = InfoBarSeverity.Warning, Title = $"{blocked.Count} protected item(s) will be skipped", Message = blocked[0].r! });
        if (noAccess.Count > 0)
            body.Children.Add(new InfoBar { IsOpen = true, IsClosable = false, Severity = InfoBarSeverity.Warning, Title = "Administrator rights may be needed", Message = $"You do not have full access to {noAccess.Count} item(s); they may fail to move. Use \"Run as admin\" to manage them." });
        body.Children.Add(new TextBlock
        {
            Text = "Items that changed since the scan are skipped automatically.",
            Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"],
            TextWrapping = TextWrapping.Wrap,
        });
        var dlg = new ContentDialog
        {
            XamlRoot = Content.XamlRoot,
            Title = "Move to Recycle Bin?",
            Content = body,
            PrimaryButtonText = "Move to Recycle Bin",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Close,
        };
        if (await dlg.ShowAsync() != ContentDialogResult.Primary) return;
        try
        {
            _session.RecycleStart(nodes, Hwnd);
            _opRunning = true;
            Busy.IsActive = true;
            StatusText.Text = "Moving to Recycle Bin…";
            _timer.Start();
        }
        catch (Exception e)
        {
            ShowInfo(InfoBarSeverity.Error, "Cannot move to Recycle Bin", e.Message);
        }
    }

    private void PollOperation()
    {
        if (_session is null) return;
        var p = _session.OpProgress();
        StatusText.Text = $"Moving to Recycle Bin… {p.done} of {p.total}";
        if (p.state != 2) return;
        _opRunning = false;
        _timer.Stop();
        Busy.IsActive = false;
        var results = _session.OpResults(p.total);
        RefreshTree();
        UpdatePanes();
        var problems = results.Where(r => r.Status != 0).Take(5).Select(r => $"{r.Path}: {r.Message}");
        ShowInfo(p.failed + p.skipped == 0 ? InfoBarSeverity.Success : InfoBarSeverity.Warning,
            $"{p.ok} moved to the Recycle Bin" + (p.skipped > 0 ? $", {p.skipped} skipped" : "") + (p.failed > 0 ? $", {p.failed} failed" : ""),
            string.Join("\n", problems));
    }

    // ------------------------------------------------------------------ snapshots and export

    private async void SaveSnapshot_Click(object sender, RoutedEventArgs e)
    {
        if (_session is null) return;
        var picker = new FileSavePicker(AppWindow.Id) { SuggestedFileName = SuggestedName() };
        picker.FileTypeChoices.Add("TreeDock scan", [".tdsnap"]);
        await PickAndRun(picker, path => _session.SaveSnapshot(path), "Saved");
    }

    private async void ExportCsv_Click(object sender, RoutedEventArgs e) => await Export(json: false);
    private async void ExportJson_Click(object sender, RoutedEventArgs e) => await Export(json: true);

    private async Task Export(bool json)
    {
        if (_session is null) return;
        var picker = new FileSavePicker(AppWindow.Id) { SuggestedFileName = SuggestedName() };
        picker.FileTypeChoices.Add(json ? "JSON" : "CSV (spreadsheet)", [json ? ".json" : ".csv"]);
        await PickAndRun(picker, path => _session.Export(path, json), "Exported");
    }

    private async Task PickAndRun(FileSavePicker picker, Action<string> act, string verb)
    {
        try
        {
            var r = await picker.PickSaveFileAsync();
            if (r is null) return;
            await Task.Run(() => act(r.Path));
            StatusText.Text = $"{verb} {r.Path}";
        }
        catch (Exception ex)
        {
            ShowInfo(InfoBarSeverity.Error, "Could not save", ex.Message);
        }
    }

    private string SuggestedName()
    {
        string root = _session?.RootPath ?? "scan";
        string clean = new(root.Select(ch => char.IsLetterOrDigit(ch) ? ch : '_').ToArray());
        return $"TreeDock {clean.Trim('_')} {DateTime.Now:yyyy-MM-dd HHmm}";
    }

    private async void OpenSnapshot_Click(object sender, RoutedEventArgs e)
    {
        try
        {
            var picker = new FileOpenPicker(AppWindow.Id);
            picker.FileTypeFilter.Add(".tdsnap");
            var r = await picker.PickSingleFileAsync();
            if (r is null) return;
            var s = Session.OpenSnapshot(r.Path);
            AttachSession(s);
            _scanPath = null;
            PathBox.Text = s.RootPath;
            SetScanning(false);
            ShowScanResult(ScanState.Done);
            Info.Title = $"Saved scan of {s.RootPath}";
        }
        catch (Exception ex)
        {
            ShowInfo(InfoBarSeverity.Error, "Could not open the scan", ex.Message);
        }
    }

    private async void About_Click(object sender, RoutedEventArgs e)
    {
        var text = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            Text = "TreeDock 0.1.0 (preview)\n\nCopyright © 2026 ASPEC RACING PTY LTD.\n\n"
                + "TreeDock is source-available under the PolyForm Noncommercial License 1.0.0. It is free for personal and other "
                + "noncommercial use. Commercial use requires permission or a commercial licence from ASPEC RACING PTY LTD.\n\n"
                + "TreeDock works offline and collects no telemetry. Third-party components are listed in THIRD_PARTY_NOTICES.md.",
        };
        await new ContentDialog { XamlRoot = Content.XamlRoot, Title = "About TreeDock", Content = text, CloseButtonText = "Close" }.ShowAsync();
    }

    private void ShowInfo(InfoBarSeverity severity, string title, string message)
    {
        Info.Severity = severity;
        Info.Title = title;
        Info.Message = message;
        Info.IsOpen = true;
    }
}
