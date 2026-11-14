using System;
using System.Collections.Concurrent;
using System.Threading;
using System.Threading.Tasks;

namespace TcTidy.VsExtension.Core.Tests
{
    /// <summary>
    /// A stand-in for the IDE's UI thread: one thread that runs, in order, the work posted to its
    /// synchronization context, so an await started on it resumes on it.
    /// </summary>
    internal sealed class UiThread : IDisposable
    {
        private readonly BlockingCollection<Action> _queue = new BlockingCollection<Action>();
        private readonly Thread _thread;

        public UiThread()
        {
            _thread = new Thread(() =>
            {
                SynchronizationContext.SetSynchronizationContext(new Context(_queue));
                foreach (var work in _queue.GetConsumingEnumerable()) work();
            })
            { IsBackground = true };
            _thread.Start();
        }

        public int Id => _thread.ManagedThreadId;

        /// <summary>Queue <paramref name="work"/> on the thread, from any thread.</summary>
        public void Post(Action work) => _queue.Add(work);

        /// <summary>Start <paramref name="work"/> on the thread; the task ends when it does.</summary>
        public Task<T> Run<T>(Func<Task<T>> work)
        {
            var done = new TaskCompletionSource<T>(TaskCreationOptions.RunContinuationsAsynchronously);
            Post(() =>
            {
                Task<T> task;
                try { task = work(); }
                catch (Exception ex)
                {
                    done.SetException(ex);
                    return;
                }
                task.ContinueWith(
                    t =>
                    {
                        if (t.IsFaulted) done.SetException(t.Exception!.InnerExceptions);
                        else if (t.IsCanceled) done.SetCanceled();
                        else done.SetResult(t.Result);
                    },
                    TaskScheduler.Default);
            });
            return done.Task;
        }

        public void Dispose()
        {
            _queue.CompleteAdding();
            _thread.Join();
            _queue.Dispose();
        }

        private sealed class Context : SynchronizationContext
        {
            private readonly BlockingCollection<Action> _queue;

            public Context(BlockingCollection<Action> queue) => _queue = queue;

            public override void Post(SendOrPostCallback callback, object? state) =>
                _queue.Add(() => callback(state));
        }
    }
}
