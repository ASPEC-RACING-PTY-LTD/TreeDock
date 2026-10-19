// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
using System.Globalization;

namespace TreeDock.Util;

internal enum SizeUnit { Auto, GB, MB, KB }

internal static class Format
{
    private static readonly string[] Binary = ["B", "KB", "MB", "GB", "TB", "PB", "EB"];

    /// <summary>Unit used for sizes (Auto picks per value).</summary>
    public static SizeUnit Unit { get; set; } = SizeUnit.Auto;

    /// <summary>Binary units with Windows-style labels (1 KB = 1024 bytes), like Explorer.</summary>
    public static string Size(ulong bytes)
    {
        switch (Unit)
        {
            case SizeUnit.GB: return (bytes / 1073741824.0).ToString("N2", CultureInfo.CurrentCulture) + " GB";
            case SizeUnit.MB: return (bytes / 1048576.0).ToString("N1", CultureInfo.CurrentCulture) + " MB";
            case SizeUnit.KB: return (bytes / 1024.0).ToString("N0", CultureInfo.CurrentCulture) + " KB";
        }
        if (bytes < 1024) return bytes.ToString("N0", CultureInfo.CurrentCulture) + " B";
        double v = bytes;
        int i = 0;
        while (v >= 1024 && i < Binary.Length - 1) { v /= 1024; i++; }
        return v.ToString(v >= 100 ? "N0" : "N1", CultureInfo.CurrentCulture) + " " + Binary[i];
    }

    public static string Count(ulong n) => n.ToString("N0", CultureInfo.CurrentCulture);

    public static string Percent(double fraction, bool always = false) =>
        fraction <= 0 && !always ? "" : (fraction * 100).ToString("N1", CultureInfo.CurrentCulture) + " %";

    public static string Date(long filetime)
    {
        if (filetime <= 0) return "";
        try { return DateTime.FromFileTimeUtc(filetime).ToLocalTime().ToString("g", CultureInfo.CurrentCulture); }
        catch (ArgumentOutOfRangeException) { return ""; }
    }

    public static string Duration(ulong ms) =>
        ms < 1000 ? $"{ms} ms" : ms < 60_000 ? $"{ms / 1000.0:N1} s" : $"{ms / 60_000} min {ms / 1000 % 60} s";
}

internal static class Log
{
    private static readonly object Gate = new();

    public static void Write(string message)
    {
        try
        {
            var dir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "TreeDock");
            Directory.CreateDirectory(dir);
            lock (Gate) File.AppendAllText(Path.Combine(dir, "treedock.log"), $"{DateTime.Now:O} {message}{Environment.NewLine}");
        }
        catch (IOException) { }
        catch (UnauthorizedAccessException) { }
    }
}
