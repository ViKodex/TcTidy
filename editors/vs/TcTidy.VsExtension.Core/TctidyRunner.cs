using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;

namespace TcTidy.VsExtension.Core
{
    /// <summary>Default <see cref="ITctidyRunner"/> backed by the tctidy executable.</summary>
    public sealed class TctidyRunner : ITctidyRunner
    {
        private static readonly UTF8Encoding Utf8NoBom = new UTF8Encoding(encoderShouldEmitUTF8Identifier: false);

        // How the paths tctidy echoes on stderr compare to the ones it was given.
        private static readonly bool WindowsPaths = RuntimeInformation.IsOSPlatform(OSPlatform.Windows);

        private readonly string _exePath;
        private readonly TimeSpan _timeout;
        private readonly string? _workingDirectory;

        /// <param name="workingDirectory">
        /// Where tctidy runs. Text formatted through stdin without a source path uses the
        /// <c>.tctidy.toml</c> found from this directory, so pass the solution directory; files on
        /// disk, and text with a source path, use the one that applies to their file. <c>null</c>
        /// keeps the caller's current directory (Visual Studio's own, never the solution's).
        /// </param>
        public TctidyRunner(string exePath, TimeSpan timeout, string? workingDirectory = null)
        {
            _exePath = exePath ?? throw new ArgumentNullException(nameof(exePath));
            _timeout = timeout;
            _workingDirectory = workingDirectory;
        }

        public string FormatText(string source, StSection section, string? sourcePath = null)
        {
            if (source == null) throw new ArgumentNullException(nameof(source));

            var args = "-";
            if (section == StSection.Declaration) args += " --decl";
            else if (section == StSection.Implementation) args += " --impl";
            if (!string.IsNullOrEmpty(sourcePath)) args += " --stdin-filepath " + CommandLine.Quote(sourcePath!);

            var (exitCode, stdout, stderr) = Run(args, source);
            if (exitCode == 0) return stdout;
            throw new FormatterException(exitCode, Describe(stderr, stdout));
        }

        public string FormatXml(string xml)
        {
            if (xml == null) throw new ArgumentNullException(nameof(xml));

            // `tctidy -` auto-detects TwinCAT XML on stdin and reformats the ST inside,
            // preserving every byte outside it; the formatted document comes back on stdout.
            var (exitCode, stdout, stderr) = Run("-", xml);
            if (exitCode == 0) return stdout;
            throw new FormatterException(exitCode, Describe(stderr, stdout));
        }

        public FormatResult FormatFile(string path)
        {
            if (path == null) throw new ArgumentNullException(nameof(path));

            var before = ReadAllBytes(path);
            var (exitCode, stdout, stderr) = Run(CommandLine.Quote(path), stdin: null);
            if (exitCode != 0) throw new FormatterException(exitCode, Describe(stderr, stdout));

            var after = ReadAllBytes(path);
            return new FormatResult(!BytesEqual(before, after));
        }

        public BatchOutcome FormatFiles(IReadOnlyList<string> paths)
        {
            if (paths == null) throw new ArgumentNullException(nameof(paths));
            if (paths.Count == 0) return new BatchOutcome(0, 0, Array.Empty<FileFailure>());

            // Snapshot each file's content before the run so the outcome can report changed vs
            // unchanged afterwards (tctidy itself emits only an aggregate exit code). A
            // file that cannot even be read is a failure and is not handed to tctidy.
            var failures = new List<FileFailure>();
            var before = new Dictionary<string, byte[]>(paths.Count);
            var readable = new List<string>(paths.Count);
            foreach (var path in paths)
            {
                try
                {
                    before[path] = Hash(ReadAllBytes(path));
                    readable.Add(path);
                }
                catch (FormatterException ex)
                {
                    failures.Add(new FileFailure(path, ex.Message));
                }
            }

            if (readable.Count == 0) return new BatchOutcome(0, 0, failures);

            // One process for the whole batch. The path list goes on stdin
            // (`--files-from -`), so the command line stays tiny no matter how many files
            // there are, passing them as arguments would blow past the OS command-line
            // length limit (~32 KB on Windows) on a large project.
            EngineRun run;
            try
            {
                var (exitCode, _, stderr) = Run("--files-from -", string.Join("\n", readable));
                run = EngineRun.Exited(exitCode, stderr);
            }
            catch (Exception ex)
            {
                // Not started, timed out, or a broken pipe: no exit code to read, but tctidy may
                // already have rewritten some files, so every file is still re-read and reported.
                run = EngineRun.Aborted(
                    ex is FormatterException ? ex.Message : "tctidy failed: " + ex.Message);
            }

            // Re-read each file: a changed file was rewritten (its content differs), since
            // tctidy only writes when it changes something. DiskBatchResult sorts the rest
            // by how the run ended and by the exact paths named in its stderr.
            var states = new List<(string Path, bool Changed)>(readable.Count);
            foreach (var path in readable)
            {
                byte[] after;
                try
                {
                    after = ReadAllBytes(path);
                }
                catch (FormatterException ex)
                {
                    failures.Add(new FileFailure(path, ex.Message));
                    continue;
                }
                states.Add((path, !BytesEqual(before[path], Hash(after))));
            }

            var outcome = DiskBatchResult.Build(states, run, WindowsPaths);
            failures.AddRange(outcome.Failures);
            return new BatchOutcome(outcome.Changed, outcome.Unchanged, failures);
        }

        private (int exitCode, string stdout, string stderr) Run(string arguments, string? stdin)
        {
            var psi = new ProcessStartInfo
            {
                FileName = _exePath,
                Arguments = arguments,
                UseShellExecute = false,
                CreateNoWindow = true,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                RedirectStandardInput = stdin != null,
                StandardOutputEncoding = Utf8NoBom,
                StandardErrorEncoding = Utf8NoBom,
            };
            if (_workingDirectory != null && Directory.Exists(_workingDirectory))
            {
                psi.WorkingDirectory = _workingDirectory;
            }

            using var process = new Process { StartInfo = psi };
            try
            {
                process.Start();
            }
            catch (Exception e)
            {
                throw new FormatterException(-1, $"could not start tctidy ('{_exePath}'): {e.Message}");
            }

            // Write stdin as UTF-8 without a BOM (writing to BaseStream avoids the
            // console encoding adding one), then close it so tctidy sees EOF.
            if (stdin != null)
            {
                var bytes = Utf8NoBom.GetBytes(stdin);
                process.StandardInput.BaseStream.Write(bytes, 0, bytes.Length);
                process.StandardInput.BaseStream.Flush();
                process.StandardInput.Close();
            }

            // Read both pipes asynchronously to avoid a full-buffer deadlock.
            var stdoutTask = process.StandardOutput.ReadToEndAsync();
            var stderrTask = process.StandardError.ReadToEndAsync();

            if (!process.WaitForExit((int)_timeout.TotalMilliseconds))
            {
                // Wait for the kill to land, so no file is rewritten after the caller re-reads it.
                try { process.Kill(); process.WaitForExit(5000); } catch { /* already exiting */ }
                throw new FormatterException(-1, $"tctidy timed out after {_timeout.TotalSeconds:0}s");
            }
            process.WaitForExit(); // let the async readers drain after exit

            return (process.ExitCode, stdoutTask.GetAwaiter().GetResult(), stderrTask.GetAwaiter().GetResult());
        }

        private static string Describe(string stderr, string stdout)
        {
            var message = string.IsNullOrWhiteSpace(stderr) ? stdout : stderr;
            if (string.IsNullOrWhiteSpace(message)) return "tctidy failed";
            message = message.Trim();
            // tctidy prefixes its own diagnostics with "tctidy: "; drop it here, the
            // dialog already attributes the failure to TcTidy and names the file, so the
            // prefix is redundant (e.g. "FB_X: tctidy: parse error …" → "FB_X: parse error …").
            const string prefix = "tctidy: ";
            if (message.StartsWith(prefix, StringComparison.Ordinal))
            {
                message = message.Substring(prefix.Length);
            }
            return message;
        }

        // Map read failures (e.g. TwinCAT still holding the file open, a vanished or
        // unreadable file) to a FormatterException, mirroring the spawn/timeout mapping, so the
        // caller treats them as a per-file failure rather than an unhandled, batch-aborting throw.
        private static byte[] ReadAllBytes(string path)
        {
            try
            {
                return File.ReadAllBytes(path);
            }
            catch (Exception e) when (e is IOException || e is UnauthorizedAccessException)
            {
                throw new FormatterException(-1, $"could not read '{path}': {e.Message}");
            }
        }

        // A compact content fingerprint, so the batch keeps a "before" snapshot of every
        // file without holding all of their bytes in memory at once.
        private static byte[] Hash(byte[] bytes)
        {
            using var sha = SHA256.Create();
            return sha.ComputeHash(bytes);
        }

        private static bool BytesEqual(byte[] a, byte[] b)
        {
            if (a.Length != b.Length) return false;
            for (var i = 0; i < a.Length; i++)
            {
                if (a[i] != b[i]) return false;
            }
            return true;
        }
    }
}
