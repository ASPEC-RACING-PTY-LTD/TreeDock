// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Sunburst chart rendered with Win2D. The engine computes the arcs; this
// control draws them, hit-tests by angle and radius, and shows the selected
// folder's name and size in the centre.

using System.Numerics;
using Microsoft.Graphics.Canvas;
using Microsoft.Graphics.Canvas.Geometry;
using Microsoft.Graphics.Canvas.Text;
using Microsoft.Graphics.Canvas.UI.Xaml;
using Microsoft.UI;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using TreeDock.Engine;
using TreeDock.Util;
using Windows.UI;

namespace TreeDock.Controls;

public sealed partial class SunburstView : UserControl
{
    private const int Rings = 5;
    private readonly CanvasControl _canvas;
    private Session? _session;
    private uint _root;
    private uint _selected = uint.MaxValue;
    private readonly SunburstArc[] _arcs = new SunburstArc[60_000];
    private int _count;
    private bool _dirty = true;
    private readonly Dictionary<ushort, Color> _extColors = new();
    private readonly CanvasTextFormat _center = new() { FontSize = 14, HorizontalAlignment = CanvasHorizontalAlignment.Center, WordWrapping = CanvasWordWrapping.NoWrap, TrimmingGranularity = CanvasTextTrimmingGranularity.Character };
    private readonly CanvasTextFormat _centerSize = new() { FontSize = 20, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold, HorizontalAlignment = CanvasHorizontalAlignment.Center };

    public event Action<uint>? Hovered;
    public event Action<uint>? NodeClicked;
    public event Action<uint>? NodeActivated;

    public SunburstView()
    {
        _canvas = new CanvasControl();
        _canvas.Draw += OnDraw;
        _canvas.SizeChanged += (_, _) => _canvas.Invalidate();
        _canvas.PointerMoved += (_, e) => Hovered?.Invoke(Hit(e.GetCurrentPoint(_canvas).Position) ?? uint.MaxValue);
        _canvas.PointerExited += (_, _) => Hovered?.Invoke(uint.MaxValue);
        _canvas.Tapped += (_, e) => { if (Hit(e.GetPosition(_canvas)) is uint n) NodeClicked?.Invoke(n); };
        _canvas.DoubleTapped += (_, e) => { if (Hit(e.GetPosition(_canvas)) is uint n) NodeActivated?.Invoke(n); };
        Content = _canvas;
        Unloaded += (_, _) => _canvas.RemoveFromVisualTree();
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(this,
            "Sunburst chart of the selected folder. The details list provides the same information.");
    }

    internal void SetData(Session? session, uint root, uint selected)
    {
        if (!ReferenceEquals(session, _session)) _extColors.Clear();
        _session = session;
        _root = root;
        _selected = selected;
        _dirty = true;
        _canvas.Invalidate();
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

    private (Vector2 c, float r0, float ring) Geometry()
    {
        float w = (float)_canvas.ActualWidth, h = (float)_canvas.ActualHeight;
        float radius = MathF.Max(10, MathF.Min(w, h) / 2 - 8);
        float r0 = radius * 0.28f;
        return (new Vector2(w / 2, h / 2), r0, (radius - r0) / Rings);
    }

    private void OnDraw(CanvasControl sender, CanvasDrawEventArgs args)
    {
        var ds = args.DrawingSession;
        ds.Clear(Colors.Transparent);
        if (_session is null) return;
        if (_dirty)
        {
            _dirty = false;
            _count = _session.Sunburst(_root, Rings, 0.002f, _arcs);
        }
        var (c, r0, ring) = Geometry();
        bool dark = ActualTheme == ElementTheme.Dark;
        var stroke = dark ? Color.FromArgb(255, 32, 32, 32) : Colors.White;
        int selectedIndex = -1;
        for (int i = 0; i < _count; i++)
        {
            ref readonly var a = ref _arcs[i];
            float inner = r0 + a.depth * ring, outer = inner + ring - 1;
            Color fill = (a.flags & 2) != 0 ? (dark ? Color.FromArgb(255, 70, 70, 70) : Color.FromArgb(255, 200, 200, 200))
                : (a.flags & 1) != 0 ? DirColor(a, dark) : ExtColor(a.ext);
            using var geo = Segment(sender, c, inner, outer, a.start, a.sweep);
            ds.FillGeometry(geo, fill);
            ds.DrawGeometry(geo, stroke, 1);
            if (a.node == _selected && (a.flags & 2) == 0) selectedIndex = i;
        }
        if (selectedIndex >= 0)
        {
            ref readonly var s = ref _arcs[selectedIndex];
            float inner = r0 + s.depth * ring;
            using var geo = Segment(sender, c, inner, inner + ring - 1, s.start, s.sweep);
            ds.DrawGeometry(geo, (Color)Application.Current.Resources["SystemAccentColor"], 3);
        }
        // Centre: the folder shown.
        var info = _session.Node(_root);
        var textColor = dark ? Colors.White : Colors.Black;
        ds.DrawText(Format.Size(info.allocated), new Vector2(c.X, c.Y - 18), textColor, _centerSize);
        using var nameLayout = new CanvasTextLayout(sender, _session.Name(_root), _center, r0 * 1.7f, 20) { Options = CanvasDrawTextOptions.Clip };
        ds.DrawTextLayout(nameLayout, c.X - r0 * 0.85f, c.Y + 10, textColor);
    }

    private static CanvasGeometry Segment(ICanvasResourceCreator rc, Vector2 c, float inner, float outer, float start, float sweep)
    {
        if (sweep >= 0.9999f)
        {
            // Full ring: two concentric circles combined.
            using var o = CanvasGeometry.CreateEllipse(rc, c, outer, outer);
            using var i = CanvasGeometry.CreateEllipse(rc, c, inner, inner);
            return o.CombineWith(i, Matrix3x2.Identity, CanvasGeometryCombine.Exclude);
        }
        float a0 = start * MathF.Tau - MathF.PI / 2, a1 = (start + sweep) * MathF.Tau - MathF.PI / 2;
        Vector2 P(float r, float a) => c + new Vector2(MathF.Cos(a) * r, MathF.Sin(a) * r);
        var size = sweep > 0.5f ? CanvasArcSize.Large : CanvasArcSize.Small;
        using var pb = new CanvasPathBuilder(rc);
        pb.BeginFigure(P(outer, a0));
        pb.AddArc(P(outer, a1), outer, outer, 0, CanvasSweepDirection.Clockwise, size);
        pb.AddLine(P(inner, a1));
        pb.AddArc(P(inner, a0), inner, inner, 0, CanvasSweepDirection.CounterClockwise, size);
        pb.EndFigure(CanvasFigureLoop.Closed);
        return CanvasGeometry.CreatePath(pb);
    }

    private uint? Hit(Windows.Foundation.Point p)
    {
        var (c, r0, ring) = Geometry();
        var d = new Vector2((float)p.X, (float)p.Y) - c;
        float r = d.Length();
        if (r < r0) return _root;
        int depth = (int)((r - r0) / ring);
        float ang = MathF.Atan2(d.Y, d.X) + MathF.PI / 2;
        if (ang < 0) ang += MathF.Tau;
        float t = ang / MathF.Tau;
        for (int i = 0; i < _count; i++)
        {
            ref readonly var a = ref _arcs[i];
            if (a.depth == depth && t >= a.start && t < a.start + a.sweep && (a.flags & 2) == 0) return a.node;
        }
        return null;
    }

    private Color ExtColor(ushort ext)
    {
        if (_extColors.TryGetValue(ext, out var col)) return col;
        col = FileCategories.ColorFor(_session?.ExtName(ext) ?? "");
        _extColors[ext] = col;
        return col;
    }

    private static Color DirColor(in SunburstArc a, bool dark)
    {
        // Hue follows the angle so neighbouring folders differ; lightness follows depth.
        double hue = (a.start + a.sweep / 2) * 360;
        double l = (dark ? 0.42 : 0.62) + a.depth * 0.05;
        return Hsl(hue, 0.45, Math.Min(l, 0.8));
    }

    private static Color Hsl(double h, double s, double l)
    {
        double c = (1 - Math.Abs(2 * l - 1)) * s, x = c * (1 - Math.Abs(h / 60 % 2 - 1)), m = l - c / 2;
        (double r, double g, double b) = h switch
        {
            < 60 => (c, x, 0.0), < 120 => (x, c, 0.0), < 180 => (0.0, c, x), < 240 => (0.0, x, c), < 300 => (x, 0.0, c), _ => (c, 0.0, x),
        };
        return Color.FromArgb(255, (byte)((r + m) * 255), (byte)((g + m) * 255), (byte)((b + m) * 255));
    }
}
