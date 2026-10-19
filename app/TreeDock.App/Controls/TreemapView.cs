// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Treemap rendered with Win2D. Layout is computed by the engine
// (squarified, small items merged); this control only draws and hit-tests.

using Microsoft.Graphics.Canvas;
using Microsoft.Graphics.Canvas.Text;
using Microsoft.Graphics.Canvas.UI.Xaml;
using Microsoft.UI;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using TreeDock.Engine;
using Windows.UI;

namespace TreeDock.Controls;

public sealed partial class TreemapView : UserControl
{
    private readonly CanvasControl _canvas;
    private Session? _session;
    private uint _root;
    private uint _selected = uint.MaxValue;
    private TreemapRect[] _rects = new TreemapRect[200_000];
    private int _count;
    private bool _dirty = true;
    private readonly Dictionary<ushort, Color> _extColors = new();
    private readonly CanvasTextFormat _label = new() { FontSize = 11, WordWrapping = CanvasWordWrapping.NoWrap, TrimmingGranularity = CanvasTextTrimmingGranularity.Character };

    /// <summary>Raised with the node under the pointer (or uint.MaxValue).</summary>
    public event Action<uint>? Hovered;
    /// <summary>Raised when a rectangle is clicked.</summary>
    public event Action<uint>? NodeClicked;
    /// <summary>Raised when a directory rectangle is double-clicked (drill down).</summary>
    public event Action<uint>? NodeActivated;

    public TreemapView()
    {
        _canvas = new CanvasControl();
        _canvas.Draw += OnDraw;
        _canvas.SizeChanged += (_, _) => { _dirty = true; _canvas.Invalidate(); };
        _canvas.PointerMoved += OnPointerMoved;
        _canvas.PointerExited += (_, _) => Hovered?.Invoke(uint.MaxValue);
        _canvas.Tapped += (_, e) => { if (HitNode(e.GetPosition(_canvas)) is uint n) NodeClicked?.Invoke(n); };
        _canvas.DoubleTapped += (_, e) => { if (HitNode(e.GetPosition(_canvas)) is uint n) NodeActivated?.Invoke(n); };
        Content = _canvas;
        Unloaded += (_, _) => { _canvas.RemoveFromVisualTree(); };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(this,
            "Treemap. A visual overview of the selected folder; the folder list provides the same information.");
    }

    internal void SetData(Session? session, uint root, uint selected)
    {
        if (!ReferenceEquals(session, _session)) _extColors.Clear();
        _session = session;
        _root = root;
        _selected = selected;
        Refresh();
    }

    internal void SetSelected(uint node)
    {
        _selected = node;
        _canvas.Invalidate();
    }

    public void Refresh()
    {
        _dirty = true;
        _canvas.Invalidate();
    }

    private void Relayout()
    {
        _dirty = false;
        _count = 0;
        if (_session is null || _canvas.ActualWidth < 4 || _canvas.ActualHeight < 4) return;
        var opts = new TdTreemapOptions
        {
            width = (float)_canvas.ActualWidth,
            height = (float)_canvas.ActualHeight,
            min_px = 3,
            padding = 2,
            header = 0,
            max_depth = 24,
        };
        _count = _session.Treemap(_root, opts, _rects);
    }

    private void OnDraw(CanvasControl sender, CanvasDrawEventArgs args)
    {
        if (_dirty) Relayout();
        var ds = args.DrawingSession;
        bool dark = ActualTheme == ElementTheme.Dark;
        var dirFill = dark ? Color.FromArgb(255, 32, 32, 32) : Color.FromArgb(255, 236, 236, 236);
        var border = dark ? Color.FromArgb(255, 18, 18, 18) : Color.FromArgb(255, 255, 255, 255);
        ds.Clear(Colors.Transparent);
        TreemapRect? sel = null;
        int labels = 0;
        for (int i = 0; i < _count; i++)
        {
            ref readonly var r = ref _rects[i];
            bool isDir = (r.flags & 1) != 0;
            bool other = (r.flags & 2) != 0;
            Color fill = other ? (dark ? Color.FromArgb(255, 70, 70, 70) : Color.FromArgb(255, 190, 190, 190))
                : isDir ? dirFill : ColorFor(r.ext);
            ds.FillRectangle(r.x, r.y, r.w, r.h, fill);
            if (other && r.w > 6 && r.h > 6)
            {
                // Hatch marks an aggregate of items too small to draw individually.
                var hatch = Shade(fill, dark ? 1.35 : 0.8);
                using var clip = ds.CreateLayer(1f, new Windows.Foundation.Rect(r.x, r.y, r.w, r.h));
                for (float d = -r.h; d < r.w; d += 6)
                    ds.DrawLine(r.x + d, r.y + r.h, r.x + d + r.h, r.y, hatch, 1);
            }
            if (!isDir && r.w > 3 && r.h > 3)
            {
                // Simple bevel: lighter top edge, darker bottom edge.
                ds.FillRectangle(r.x, r.y, r.w, 1, Shade(fill, 1.25));
                ds.FillRectangle(r.x, r.y + r.h - 1, r.w, 1, Shade(fill, 0.7));
            }
            if (r.w >= 5 && r.h >= 5) ds.DrawRectangle(r.x, r.y, r.w, r.h, border, 1);
            if (r.node == _selected && !other) sel = r;
            if (!isDir && !other && r.w > 70 && r.h > 18 && labels < 300 && _session is not null)
            {
                labels++;
                using var layout = new CanvasTextLayout(sender, _session.Name(r.node), _label, r.w - 6, 14) { Options = CanvasDrawTextOptions.Clip };
                ds.DrawTextLayout(layout, r.x + 3, r.y + 2, Luma(fill) > 0.55 ? Colors.Black : Colors.White);
            }
        }
        if (sel is TreemapRect s)
        {
            var accent = (Color)Application.Current.Resources["SystemAccentColor"];
            ds.DrawRectangle(s.x + 1, s.y + 1, Math.Max(0, s.w - 2), Math.Max(0, s.h - 2), accent, 3);
        }
    }

    private uint? HitNode(Windows.Foundation.Point p)
    {
        float x = (float)p.X, y = (float)p.Y;
        int best = -1;
        for (int i = 0; i < _count; i++)
        {
            ref readonly var r = ref _rects[i];
            if (x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h && (best < 0 || r.depth >= _rects[best].depth) && (r.flags & 2) == 0)
                best = i;
        }
        return best < 0 ? null : _rects[best].node;
    }

    private void OnPointerMoved(object sender, PointerRoutedEventArgs e) =>
        Hovered?.Invoke(HitNode(e.GetCurrentPoint(_canvas).Position) ?? uint.MaxValue);

    private Color ColorFor(ushort ext)
    {
        if (_extColors.TryGetValue(ext, out var c)) return c;
        c = FileCategories.ColorFor(_session?.ExtName(ext) ?? "");
        _extColors[ext] = c;
        return c;
    }

    private static Color Shade(Color c, double f) =>
        Color.FromArgb(c.A, (byte)Math.Clamp(c.R * f, 0, 255), (byte)Math.Clamp(c.G * f, 0, 255), (byte)Math.Clamp(c.B * f, 0, 255));

    private static double Luma(Color c) => (0.2126 * c.R + 0.7152 * c.G + 0.0722 * c.B) / 255;
}

/// <summary>Groups extensions into categories with distinct colours (also used by the legend).</summary>
internal static class FileCategories
{
    public sealed record Category(string Name, Color Color, string[] Extensions);

    public static readonly Category[] All =
    [
        new("Video", Color.FromArgb(255, 214, 96, 77), ["mp4", "mkv", "avi", "mov", "wmv", "webm", "m4v", "mpg", "mpeg", "ts", "m2ts", "flv", "vob"]),
        new("Audio", Color.FromArgb(255, 228, 160, 60), ["mp3", "flac", "wav", "aac", "ogg", "m4a", "wma", "opus", "aiff", "alac"]),
        new("Images", Color.FromArgb(255, 214, 196, 74), ["jpg", "jpeg", "png", "gif", "bmp", "tif", "tiff", "webp", "heic", "heif", "raw", "cr2", "cr3", "nef", "arw", "dng", "psd", "svg", "ico", "avif"]),
        new("Archives & images", Color.FromArgb(255, 150, 110, 200), ["zip", "rar", "7z", "tar", "gz", "bz2", "xz", "zst", "iso", "vhd", "vhdx", "vmdk", "img", "cab", "msi", "msix", "appx", "wim", "esd", "dmg"]),
        new("Documents", Color.FromArgb(255, 82, 156, 214), ["pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "rtf", "odt", "ods", "odp", "md", "csv", "epub", "one", "pst", "ost"]),
        new("Code & build", Color.FromArgb(255, 86, 180, 140), ["rs", "cs", "cpp", "cc", "c", "h", "hpp", "js", "mjs", "ts", "tsx", "py", "java", "go", "json", "xml", "yml", "yaml", "html", "css", "pdb", "obj", "lib", "o", "a", "class", "jar", "rlib", "rmeta", "pch", "ipch", "node", "wasm"]),
        new("Programs & system", Color.FromArgb(255, 120, 132, 150), ["exe", "dll", "sys", "drv", "mui", "ocx", "cpl", "so", "dylib", "efi", "bin", "dat", "etl", "log", "evtx", "db", "sqlite", "edb", "cat", "manifest"]),
    ];

    private static readonly Dictionary<string, Category> ByExt =
        All.SelectMany(c => c.Extensions.Select(e => (e, c))).GroupBy(x => x.e).ToDictionary(g => g.Key, g => g.First().c);

    public static Category? Of(string ext) => ByExt.TryGetValue(ext, out var c) ? c : null;

    public static Color ColorFor(string ext)
    {
        if (Of(ext) is Category c) return c.Color;
        // Stable muted colour for uncategorised extensions.
        uint h = 2166136261;
        foreach (char ch in ext) h = (h ^ ch) * 16777619;
        double hue = h % 360;
        return FromHsl(hue, 0.25, ext.Length == 0 ? 0.42 : 0.5);
    }

    private static Color FromHsl(double h, double s, double l)
    {
        double c = (1 - Math.Abs(2 * l - 1)) * s, x = c * (1 - Math.Abs(h / 60 % 2 - 1)), m = l - c / 2;
        (double r, double g, double b) = h switch
        {
            < 60 => (c, x, 0.0), < 120 => (x, c, 0.0), < 180 => (0.0, c, x), < 240 => (0.0, x, c), < 300 => (x, 0.0, c), _ => (c, 0.0, x),
        };
        return Color.FromArgb(255, (byte)((r + m) * 255), (byte)((g + m) * 255), (byte)((b + m) * 255));
    }
}
