// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Minimal vertical splitter: drag with the mouse, or focus it and use the
// Left/Right arrow keys. Adjusts the width of the grid column to its left.

using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Windows.System;

namespace TreeDock.Controls;

public sealed partial class Splitter : Grid
{
    private double _startX;
    private double _startWidth;
    private bool _dragging;

    public Splitter()
    {
        Width = 12;
        Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent);
        IsTabStop = true;
        UseSystemFocusVisuals = true;
        ProtectedCursor = InputSystemCursor.Create(InputSystemCursorShape.SizeWestEast);
        AutomationProperties.SetName(this, "Resize panes");
        Children.Add(new Microsoft.UI.Xaml.Shapes.Rectangle
        {
            Width = 2,
            HorizontalAlignment = HorizontalAlignment.Center,
            Margin = new Thickness(0, 24, 0, 24),
            RadiusX = 1,
            RadiusY = 1,
            Fill = (Brush)Application.Current.Resources["ControlStrongStrokeColorDefaultBrush"],
            Opacity = 0.35,
        });
        PointerPressed += OnPressed;
        PointerMoved += OnMoved;
        PointerReleased += (_, e) => { _dragging = false; ReleasePointerCapture(e.Pointer); };
        PointerCaptureLost += (_, _) => _dragging = false;
        KeyDown += OnKey;
    }

    private ColumnDefinition? LeftColumn =>
        Parent is Grid g && GetColumn(this) > 0 ? g.ColumnDefinitions[GetColumn(this) - 1] : null;

    private void OnPressed(object sender, PointerRoutedEventArgs e)
    {
        if (LeftColumn is not ColumnDefinition col) return;
        _dragging = CapturePointer(e.Pointer);
        _startX = e.GetCurrentPoint(Parent as UIElement).Position.X;
        _startWidth = col.ActualWidth;
        Focus(FocusState.Pointer);
    }

    private void OnMoved(object sender, PointerRoutedEventArgs e)
    {
        if (!_dragging) return;
        SetWidth(_startWidth + e.GetCurrentPoint(Parent as UIElement).Position.X - _startX);
    }

    private void OnKey(object sender, KeyRoutedEventArgs e)
    {
        if (LeftColumn is not ColumnDefinition col) return;
        double step = e.Key switch { VirtualKey.Left => -24, VirtualKey.Right => 24, _ => 0 };
        if (step == 0) return;
        SetWidth(col.ActualWidth + step);
        e.Handled = true;
    }

    private void SetWidth(double w)
    {
        if (LeftColumn is not ColumnDefinition col || Parent is not Grid g) return;
        double max = g.ActualWidth - 300;
        col.Width = new GridLength(Math.Clamp(w, col.MinWidth, Math.Max(col.MinWidth, max)));
    }
}
