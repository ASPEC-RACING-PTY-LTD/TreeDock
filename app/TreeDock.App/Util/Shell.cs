// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
using System.Diagnostics;
using System.Runtime.InteropServices;

namespace TreeDock.Util;

/// <summary>Windows shell actions on paths chosen by the user.</summary>
internal static partial class Shell
{
    private const uint SHOP_FILEPATH = 0x2;

    [LibraryImport("shell32.dll", StringMarshalling = StringMarshalling.Utf16)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool SHObjectProperties(nint hwnd, uint shopObjectType, string pszObjectName, string? pszPropertyPage);

    /// <summary>Opens the item with its default application (or opens a folder in File Explorer).</summary>
    public static void Open(string path) => Run(() => Process.Start(new ProcessStartInfo(path) { UseShellExecute = true }));

    /// <summary>Opens File Explorer at the item's location with the item selected.</summary>
    public static void Reveal(string path)
    {
        if (string.IsNullOrEmpty(path)) return;
        Run(() => Process.Start(new ProcessStartInfo("explorer.exe") { ArgumentList = { "/select,", path }, UseShellExecute = false }));
    }

    /// <summary>Shows the standard Windows Properties dialog.</summary>
    public static void ShowProperties(nint hwnd, string path)
    {
        if (!string.IsNullOrEmpty(path) && !SHObjectProperties(hwnd, SHOP_FILEPATH, path, null))
            Log.Write($"Properties failed for {path}");
    }

    private static void Run(Action a)
    {
        if (App.TestMode)
        {
            // Never open other windows from automated tests.
            Log.Write("Shell action suppressed in test mode");
            return;
        }
        try { a(); }
        catch (Exception e) { Log.Write($"Shell action failed: {e.Message}"); }
    }
}
