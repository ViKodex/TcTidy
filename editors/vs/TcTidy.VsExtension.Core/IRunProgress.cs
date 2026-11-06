using System.Threading.Tasks;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// The in-flight feedback of a project-wide run (the status-bar spinner and message in the
    /// extension). <see cref="ProjectFormatRun"/> calls <see cref="EndAsync"/> exactly once, after
    /// both phases, whatever they did.
    /// </summary>
    public interface IRunProgress
    {
        /// <summary>The run starts on <paramref name="total"/> targets.</summary>
        Task StartAsync(int total);

        /// <summary>The open POUs are being formatted in place.</summary>
        Task OpenPhaseAsync(int count);

        /// <summary>The closed files are being formatted on disk.</summary>
        Task DiskPhaseAsync(int count);

        /// <summary>The run is over: stop the feedback.</summary>
        Task EndAsync();
    }
}
