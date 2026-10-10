// Original test-only executable: the downloaded binary delegates to the chosen Node replay.
// No shell, installer, credentials, or app-specific dispatch is involved.
using System;
using System.Diagnostics;
using System.Threading;
using System.Text.RegularExpressions;

class ReplayLauncher {
    static string Quote(string value) {
        value = Regex.Replace(value, @"(\\*)""", "$1$1\\\"");
        return "\"" + Regex.Replace(value, @"(\\+)$", "$1$1") + "\"";
    }

    static int Main(string[] args) {
        var start = new ProcessStartInfo();
        start.FileName = Environment.GetEnvironmentVariable("FABER_FAKE_NODE");
        start.Arguments = Quote(Environment.GetEnvironmentVariable("FABER_FAKE_SCRIPT")) + " " +
            Quote(Environment.GetEnvironmentVariable("FABER_FAKE_RECORDING"));
        foreach (var arg in args) start.Arguments += " " + Quote(arg);
        start.UseShellExecute = false;
        start.CreateNoWindow = true;
        start.RedirectStandardInput = true;
        start.RedirectStandardOutput = true;
        start.RedirectStandardError = true;
        using (var child = Process.Start(start)) {
            var input = new Thread(() => {
                try {
                    string line;
                    while ((line = Console.ReadLine()) != null) {
                        child.StandardInput.WriteLine(line);
                        child.StandardInput.Flush();
                    }
                } catch (System.IO.IOException) {
                } finally {
                    try { child.StandardInput.Close(); } catch { }
                }
            });
            input.IsBackground = true;
            input.Start();
            var error = new Thread(() => {
                try {
                    string line;
                    while ((line = child.StandardError.ReadLine()) != null) Console.Error.WriteLine(line);
                } catch { }
            });
            error.Start();
            try {
                string output;
                while ((output = child.StandardOutput.ReadLine()) != null) Console.WriteLine(output);
            } catch { }
            if (!child.WaitForExit(3000)) {
                try { child.Kill(); } catch { }
            }
            try { error.Join(1000); } catch { }
            return child.HasExited ? child.ExitCode : 0;
        }
    }
}
