# Distributed Colony Analytics Dashboard

Interactive Streamlit dashboard for visualizing colony analytics from parquet files.

## Features

- **Colony Selection**: Dropdown to select which colony to visualize
- **Event Filtering**: Checkboxes to show/hide specific event types
- **Interactive Charts**: Click on data points to see detailed information
- **All Visualizations**: 
  - Creature coverage percentage
  - Events timeline with markers
  - Health, Food, and Age metrics
  - Creature traits (size, kill ratio, move ratio)
  - Creature count by color

## Installation

1. Install dependencies:
```bash
cd bi
pip install -r requirements.txt
```

## Running the Dashboard

From the repository root:

```bash
streamlit run bi/dashboard.py
```

Or from the `bi` directory:

```bash
streamlit run dashboard.py
```

The dashboard will open in your browser at `http://localhost:8501`

## Usage

1. **Select Colony**: Use the dropdown in the sidebar to choose which colony to visualize
2. **Filter Events**: Use the checkboxes in the sidebar to show/hide specific event types
3. **Interact with Charts**:
   - **Hover**: Hover over any point to see detailed information in tooltips
   - **Click**: Click on data points to see expanded details (if supported by your Streamlit version)
   - **Zoom**: Use the toolbar (top-right of charts) to zoom, pan, and reset views
   - **Select**: Use the "Select" tool in the chart toolbar to select multiple points

## Data Requirements

The dashboard expects parquet files in the following structure:

```
output/bi/
  <colony_id>/
    stats.parquet
    events.parquet  (optional)
```

Generate these files by running `ingest_bi.py` from the repository root.

## Troubleshooting

- **No colonies found**: Make sure you've run `ingest_bi.py` to generate parquet files
- **Charts not loading**: Check that the parquet files contain the expected columns
- **Events not showing**: Verify that `events.parquet` exists for the selected colony

