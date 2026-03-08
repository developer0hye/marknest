export function sanitizeBaselineReport(report, entry) {
  const sanitizedOutputs = Array.isArray(report.outputs)
    ? report.outputs.map((output) => ({
        ...output,
        output_path: "output.pdf",
      }))
    : [];

  return {
    ...report,
    input_path: entry.readmePath,
    outputs: sanitizedOutputs,
  };
}

export function sanitizeBaselineMetrics(metrics) {
  const advisories = Array.isArray(metrics.advisories)
    ? metrics.advisories.filter((advisory) => advisory !== "baseline_page_count_changed")
    : [];

  return {
    ...metrics,
    baseline_exists: true,
    baseline_page_count_delta: 0,
    diff_summary: [],
    advisories,
  };
}
