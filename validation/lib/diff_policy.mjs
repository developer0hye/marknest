const MINIMUM_TOKEN_COVERAGE = 0.97;

function unique(values) {
  return [...new Set(values)];
}

export function classifyValidationResult(result) {
  const hardFailures = [];
  const advisories = [];

  const convertExitCode = Number(result?.convertExitCode ?? 1);
  if (convertExitCode > 1) {
    hardFailures.push("convert_failed");
  }
  if (convertExitCode === 1) {
    advisories.push("convert_warning_exit");
  }
  if (String(result?.reportStatus ?? "") === "failure" || Number(result?.reportErrors ?? 0) > 0) {
    hardFailures.push("render_report_failed");
  }
  if ((result?.localMissingAssets?.length ?? 0) > 0) {
    hardFailures.push("local_assets_missing");
  }
  if ((result?.headingCoverage?.missingHeadings?.length ?? 0) > 0) {
    hardFailures.push("headings_missing");
  }
  if (Number(result?.tokenCoverage?.coverage ?? 0) < MINIMUM_TOKEN_COVERAGE) {
    hardFailures.push("text_coverage_below_threshold");
  }
  if ((result?.nearBlankPages?.length ?? 0) > 0) {
    hardFailures.push("near_blank_pages");
  }

  if (Number(result?.baselinePageCountDelta ?? 0) !== 0) {
    advisories.push("baseline_page_count_changed");
  }
  if ((result?.remoteAssetFailures?.length ?? 0) > 0) {
    advisories.push("remote_asset_failures");
  }

  return {
    hardFailures: unique(hardFailures),
    advisories: unique(advisories),
  };
}
