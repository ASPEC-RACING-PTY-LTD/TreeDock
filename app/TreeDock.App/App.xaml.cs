// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
using Microsoft.UI.Xaml;

namespace TreeDock;

public partial class App : Application
{
    private Window? _window;

    /// <summary>
    /// Automated UI test mode (environment variable TREEDOCK_TEST_MODE=1): the window is placed off-screen,
    /// hidden from the taskbar and Alt+Tab, never activated, and actions that would open other windows
    /// (UAC prompts, File Explorer, Properties) are simulated and logged instead.
    /// </summary>
    public static bool TestMode { get; } = Environment.GetEnvironmentVariable("TREEDOCK_TEST_MODE") == "1";

    public App()
    {
        InitializeComponent();
        UnhandledException += (_, e) => Util.Log.Write($"Unhandled: {e.Exception}");
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        var cmd = Environment.GetCommandLineArgs();
        string? path = cmd.Length > 1 ? cmd[1] : null;
        var window = new MainWindow(path);
        _window = window;
        if (TestMode)
        {
            window.AppWindow.IsShownInSwitchers = false;
            window.AppWindow.Move(new Windows.Graphics.PointInt32(-32000, -32000));
            window.AppWindow.Show(activateWindow: false);
        }
        else
        {
            window.Activate();
        }
    }
}
