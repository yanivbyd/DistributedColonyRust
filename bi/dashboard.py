#!/usr/bin/env python3
"""
Interactive Streamlit Dashboard for Distributed Colony Analytics

Features:
- Colony selection dropdown
- Checkboxes to show/hide events by type
- Interactive Plotly charts with clickable points
- All visualizations from bi_dashboard.ipynb
"""

import os
import base64
from io import BytesIO
from pathlib import Path
from typing import List, Optional, Tuple

import pandas as pd
import plotly.graph_objects as go
import plotly.express as px
from plotly.subplots import make_subplots
import streamlit as st
from PIL import Image

# Configuration
PROJECT_ROOT = Path(__file__).parent.parent
ANALYTICS_DIR = PROJECT_ROOT / "output" / "bi"
LOCAL_S3_DIR = PROJECT_ROOT / "output" / "s3" / "distributed-colony"


@st.cache_data
def discover_colonies() -> List[Tuple[str, Path]]:
    """Discover all colony directories with stats.parquet files."""
    colonies = []
    if not ANALYTICS_DIR.exists():
        return colonies
    
    for colony_dir in sorted(ANALYTICS_DIR.iterdir()):
        if colony_dir.is_dir():
            stats_file = colony_dir / "stats.parquet"
            if stats_file.exists():
                colonies.append((colony_dir.name, colony_dir))
    
    return colonies


@st.cache_data
def load_colony_data(colony_path: Path) -> Tuple[pd.DataFrame, Optional[pd.DataFrame], Optional[pd.DataFrame]]:
    """Load stats, events, and images parquet files for a colony."""
    stats_file = colony_path / "stats.parquet"
    events_file = colony_path / "events.parquet"
    images_file = colony_path / "images.parquet"
    
    df_stats = pd.read_parquet(stats_file)
    df_events = pd.read_parquet(events_file) if events_file.exists() else None
    df_images = pd.read_parquet(images_file) if images_file.exists() else None
    
    return df_stats, df_events, df_images


def get_colony_id(df: pd.DataFrame, colony_path: Path) -> str:
    """Extract colony ID from dataframe or path."""
    if "colony_id" in df.columns and len(df) > 0:
        return str(df["colony_id"].iloc[0])
    elif "colony_instance_id" in df.columns and len(df) > 0:
        return str(df["colony_instance_id"].iloc[0])
    else:
        return colony_path.name


def add_events_to_figure(fig: go.Figure, df_events: Optional[pd.DataFrame], y_min: float = 0, y_max: float = 100, row: Optional[int] = None, col: Optional[int] = None) -> None:
    """Add event vertical lines to a figure. Reusable function for all charts.
    
    Args:
        fig: Plotly figure (regular or subplot)
        df_events: Events dataframe
        y_min: Minimum y value for event lines
        y_max: Maximum y value for event lines
        row: Row number for subplots (None for regular figures)
        col: Column number for subplots (None for regular figures)
    """
    if df_events is None or len(df_events) == 0:
        return
    
    event_colors = {
        'ColonyCreated': 'white',
        'More Food': 'green',
        'Less Food': 'red',
        'Create Creature': 'white',
        'Extinction': 'black',
        'New Topography': 'white',
        'Colony Rules Change': 'purple'
    }
    
    # Filter out ColonyCreated events
    filtered_events = df_events[df_events['event_type'] != 'ColonyCreated'].copy()
    
    for _, row_data in filtered_events.iterrows():
        tick = row_data['tick']
        event_type = row_data.get('event_type', 'N/A')
        color = event_colors.get(event_type, 'gray')
        
        event_desc = row_data.get('event_description', 'N/A')
        if pd.notna(event_desc) and event_desc != 'N/A':
            desc_str = str(event_desc)
        else:
            desc_str = 'N/A'
        
        scatter = go.Scatter(
            x=[tick, tick],
            y=[y_min, y_max],
            mode='lines+markers',
            line=dict(color=color, width=2),
            marker=dict(size=6, color=color, symbol='diamond'),
            name=event_type,
            showlegend=False,
            hovertemplate=f'<b>{event_type}</b><br>Tick: {tick}<br>Description: {desc_str}<extra></extra>',
        )
        
        # Only pass row/col if both are provided (for subplots)
        if row is not None and col is not None:
            fig.add_trace(scatter, row=row, col=col)
        else:
            fig.add_trace(scatter)


def create_creature_coverage_chart(df: pd.DataFrame, df_events: Optional[pd.DataFrame] = None) -> go.Figure:
    """Create creature coverage percentage chart."""
    required_cols = {"creatures_count", "colony_width", "colony_height"}
    missing = required_cols - set(df.columns)
    if missing:
        st.error(f"Missing columns: {missing}")
        return go.Figure()
    
    plot_df = df.sort_values("tick").copy()
    plot_df["grid_cells"] = plot_df["colony_width"] * plot_df["colony_height"]
    plot_df["creature_pct"] = (plot_df["creatures_count"] / plot_df["grid_cells"]) * 100.0
    
    fig = go.Figure()
    
    # Add creature coverage area
    fig.add_trace(go.Scatter(
        x=plot_df["tick"],
        y=plot_df["creature_pct"],
        fill='tozeroy',
        mode='lines+markers',
        line=dict(width=1.5, color='blue'),
        marker=dict(size=4, color='blue'),
        fillcolor='rgba(100, 149, 237, 0.3)',
        name='Creature coverage',
        showlegend=False,
        hovertemplate='<b>Creature Coverage</b><br>Tick: %{x}<br>Coverage: %{y:.2f}%<extra></extra>',
        customdata=plot_df[["creatures_count", "grid_cells"]].values,
    ))
    
    # Add empty cells area
    fig.add_trace(go.Scatter(
        x=plot_df["tick"],
        y=[100] * len(plot_df),
        fill='tonexty',
        mode='lines',
        line=dict(width=0),
        fillcolor='rgba(255, 255, 255, 0.8)',
        name='Empty cells',
        showlegend=False,
        hoverinfo='skip',
    ))
    
    # Add event vertical lines (excluding ColonyCreated)
    add_events_to_figure(fig, df_events, y_min=0, y_max=100)
    
    fig.update_layout(
        title="Creature coverage (%)",
        xaxis_title="Tick",
        yaxis_title="Creature % of cells occupied",
        height=600,
        yaxis_range=[0, 100],
        xaxis=dict(showgrid=False),
        yaxis=dict(showgrid=False),
        template="plotly_white",
        hovermode='closest'
    )
    
    return fig


def create_events_chart(
    df_stats: pd.DataFrame,
    df_events: Optional[pd.DataFrame]
) -> Optional[go.Figure]:
    """Create events timeline chart."""
    if df_events is None or len(df_events) == 0:
        return None
    
    df_events = df_events.sort_values('tick').reset_index(drop=True)
    
    # Create figure with chart only (table displayed separately)
    fig = make_subplots(
        rows=1, cols=1,
        specs=[[{"secondary_y": True}]]
    )
    
    # Get tick range
    if 'tick' in df_stats.columns:
        min_tick = min(df_stats['tick'].min(), df_events['tick'].min() if len(df_events) > 0 else 0)
        max_tick = max(df_stats['tick'].max(), df_events['tick'].max() if len(df_events) > 0 else 0)
    else:
        min_tick = df_events['tick'].min() if len(df_events) > 0 else 0
        max_tick = df_events['tick'].max() if len(df_events) > 0 else 0
    
    # Add creature percentage plot for reference
    if 'tick' in df_stats.columns and 'creatures_count' in df_stats.columns and 'colony_width' in df_stats.columns and 'colony_height' in df_stats.columns:
        plot_df_stats = df_stats.sort_values('tick').copy()
        plot_df_stats['grid_cells'] = plot_df_stats['colony_width'] * plot_df_stats['colony_height']
        plot_df_stats['creature_pct'] = (plot_df_stats['creatures_count'] / plot_df_stats['grid_cells']) * 100.0
        
        fig.add_trace(
            go.Scatter(
                x=plot_df_stats['tick'],
                y=plot_df_stats['creature_pct'],
                mode='lines+markers',
                line=dict(color='lightblue', width=2),
                marker=dict(size=4, color='lightblue'),
                name='Creature %',
                hovertemplate='Tick: %{x}<br>Creature %: %{y:.2f}%<extra></extra>'
            ),
            row=1, col=1,
            secondary_y=True
        )
    
    # Event colors
    event_colors = {
        'ColonyCreated': 'white',
        'More Food': 'green',
        'Less Food': 'red',
        'Create Creature': 'white',
        'Extinction': 'black',
        'New Topography': 'brown',
        'Colony Rules Change': 'purple'
    }
    
    # Plot vertical lines for each event
    event_types = df_events['event_type'].unique()
    legend_shown = set()
    
    for event_type in event_types:
        event_rows = df_events[df_events['event_type'] == event_type]
        color = event_colors.get(event_type, 'gray')
        
        for _, row in event_rows.iterrows():
            tick = row['tick']
            show_legend = event_type not in legend_shown
            if show_legend:
                legend_shown.add(event_type)
            
            event_desc = row.get('event_description', 'N/A')
            if pd.notna(event_desc) and event_desc != 'N/A':
                desc_str = str(event_desc)
            else:
                desc_str = 'N/A'
            
            fig.add_trace(
                go.Scatter(
                    x=[tick, tick],
                    y=[0, 1],
                    mode='lines+markers',
                    line=dict(color=color, width=3),
                    marker=dict(size=8, color=color, symbol='diamond'),
                    name=event_type,
                    showlegend=show_legend,
                    legendgroup=event_type,
                    hovertemplate=f'<b>{event_type}</b><br>Tick: {tick}<br>Description: {desc_str}<extra></extra>',
                    customdata=[[tick, event_type, desc_str]],
                ),
                row=1, col=1
            )
    
    # Update layout
    fig.update_xaxes(title_text="", range=[min_tick - 100, max_tick + 100], row=1, col=1)
    fig.update_yaxes(title_text="", range=[0, 1], showticklabels=False, row=1, col=1)
    if 'tick' in df_stats.columns and 'creatures_count' in df_stats.columns:
        fig.update_yaxes(title_text="Creature %", range=[0, 100], row=1, col=1, secondary_y=True)
    fig.update_layout(
        height=400,
        template="plotly_white",
        title_text=f"Colony Events Timeline (Total: {len(df_events)})",
        hovermode='closest'
    )
    
    return fig


def prepare_events_table(df_events: pd.DataFrame) -> pd.DataFrame:
    """Prepare events data for display in a table."""
    table_data = []
    for _, row in df_events.iterrows():
        event_desc = row.get('event_description') or 'N/A'
        if event_desc != 'N/A' and pd.notna(event_desc):
            event_desc_str = str(event_desc)
            event_desc_display = event_desc_str[:100] + ('...' if len(event_desc_str) > 100 else '')
        else:
            event_desc_display = 'N/A'
        
        tick_val = row.get('tick', 'N/A')
        if pd.notna(tick_val) and tick_val != 'N/A':
            tick_val = int(tick_val)
        else:
            tick_val = 'N/A'
        
        event_type_val = row.get('event_type', 'N/A')
        if pd.isna(event_type_val):
            event_type_val = 'N/A'
        
        table_data.append({
            'Tick': tick_val,
            'Event Type': event_type_val,
            'Description': event_desc_display
        })
    
    return pd.DataFrame(table_data)


def create_color_chart(df: pd.DataFrame, df_events: Optional[pd.DataFrame] = None) -> go.Figure:
    """Create creature count by color chart."""
    plot_df = df.sort_values("tick").copy()
    
    # Collect all unique original colors
    all_colors = set()
    for i in range(1, 6):  # top 1-5
        col_name = f"original_color_top{i}"
        if col_name in plot_df.columns:
            colors = plot_df[col_name].dropna().unique()
            all_colors.update(colors)
    
    if not all_colors:
        fig = go.Figure()
        fig.add_annotation(
            text="No original_color data found",
            xref="paper", yref="paper",
            x=0.5, y=0.5,
            showarrow=False
        )
        return fig
    
    # Track counts for each color over time
    color_data = {}
    for color_str in all_colors:
        color_data[color_str] = []
    
    for _, row in plot_df.iterrows():
        tick = row["tick"]
        for color_str in all_colors:
            count = None
            for i in range(1, 6):
                top_col = f"original_color_top{i}"
                count_col = f"original_color_top{i}_count"
                if top_col in row and row[top_col] == color_str:
                    if count_col in row:
                        count = row[count_col]
                    break
            color_data[color_str].append((tick, count if count is not None else 0))
    
    # Create figure
    fig = go.Figure()
    
    for color_str in sorted(all_colors):
        try:
            parts = color_str.split("_")
            if len(parts) == 3:
                r = int(parts[0])
                g = int(parts[1])
                b = int(parts[2])
                rgb_color = f"rgb({r}, {g}, {b})"
                rgba_color = f"rgba({r}, {g}, {b}, 0.5)"
            else:
                rgb_color = "rgb(128, 128, 128)"
                rgba_color = "rgba(128, 128, 128, 0.5)"
        except (ValueError, IndexError):
            rgb_color = "rgb(128, 128, 128)"
            rgba_color = "rgba(128, 128, 128, 0.5)"
        
        ticks = [t for t, _ in color_data[color_str]]
        counts = [c for _, c in color_data[color_str]]
        
        if any(c > 0 for c in counts):
            fig.add_trace(go.Scatter(
                x=ticks,
                y=counts,
                mode='lines+markers',
                name=color_str,
                line=dict(width=1.5, color=rgb_color),
                marker=dict(size=4, color=rgb_color),
                fill='tozeroy',
                fillcolor=rgba_color,
                opacity=0.7,
                showlegend=False,
                hovertemplate=f'<b>Color: {color_str}</b><br>Tick: %{{x}}<br>Count: %{{y}}<extra></extra>',
                customdata=[[color_str] * len(ticks)],
            ))
    
    fig.update_layout(
        title="Creature Count by Color",
        xaxis_title="Tick",
        yaxis_title="Creature Count",
        height=500,
        template="plotly_white",
        xaxis=dict(showgrid=False),
        yaxis=dict(showgrid=False, range=[0, None]),
        hovermode='closest'
    )
    
    # Add event vertical lines
    if df_events is not None and len(df_events) > 0:
        # Get y-axis range from the figure
        # color_data contains lists of (tick, count) tuples, so extract counts
        max_count = max([max(count for _, count in counts) for _, counts in color_data.items() if any(count > 0 for _, count in counts)], default=100)
        add_events_to_figure(fig, df_events, y_min=0, y_max=max_count)
    
    return fig


def create_health_food_age_chart(df: pd.DataFrame, df_events: Optional[pd.DataFrame] = None) -> go.Figure:
    """Create health, food, and age averages chart."""
    plot_df = df.sort_values("tick").copy()
    
    fig = make_subplots(
        rows=1, cols=3,
        subplot_titles=("Health average", "Food average", "Age average"),
        horizontal_spacing=0.08,
        shared_xaxes=True,
    )
    
    window = 5
    
    # Health
    y_col_health = None
    if "health_avg" in plot_df.columns:
        y_col_health = "health_avg"
    elif "health_mean" in plot_df.columns:
        y_col_health = "health_mean"
    
    if y_col_health is not None:
        smooth_col = f"{y_col_health}_smooth"
        plot_df[smooth_col] = plot_df[y_col_health].rolling(window=window, min_periods=1, center=True).mean()
        fig.add_trace(
            go.Scatter(x=plot_df["tick"], y=plot_df[smooth_col], mode='lines+markers',
                       line=dict(width=4, color="rgba(255, 0, 0, 0.3)"), marker=dict(size=3),
                       name="Health", showlegend=False, hovertemplate='Tick: %{x}<br>Health: %{y:.2f}<extra></extra>'),
            row=1, col=1
        )
        fig.add_trace(
            go.Scatter(x=plot_df["tick"], y=plot_df[smooth_col], mode='lines+markers',
                       line=dict(width=1, color="red"), marker=dict(size=4, color="red"),
                       name="Health", showlegend=False, hovertemplate='Tick: %{x}<br>Health: %{y:.2f}<extra></extra>'),
            row=1, col=1
        )
        fig.update_yaxes(title_text="Health", row=1, col=1, range=[0, None])
    
    # Food
    y_col_food = None
    if "food_avg" in plot_df.columns:
        y_col_food = "food_avg"
    elif "food_mean" in plot_df.columns:
        y_col_food = "food_mean"
    
    if y_col_food is not None:
        smooth_col = f"{y_col_food}_smooth"
        plot_df[smooth_col] = plot_df[y_col_food].rolling(window=window, min_periods=1, center=True).mean()
        fig.add_trace(
            go.Scatter(x=plot_df["tick"], y=plot_df[smooth_col], mode='lines+markers',
                       line=dict(width=4, color="rgba(0, 128, 0, 0.3)"), marker=dict(size=3),
                       name="Food", showlegend=False, hovertemplate='Tick: %{x}<br>Food: %{y:.2f}<extra></extra>'),
            row=1, col=2
        )
        fig.add_trace(
            go.Scatter(x=plot_df["tick"], y=plot_df[smooth_col], mode='lines+markers',
                       line=dict(width=1, color="green"), marker=dict(size=4, color="green"),
                       name="Food", showlegend=False, hovertemplate='Tick: %{x}<br>Food: %{y:.2f}<extra></extra>'),
            row=1, col=2
        )
        fig.update_yaxes(title_text="Food", row=1, col=2, range=[0, None])
    
    # Age
    y_col_age = None
    if "age_avg" in plot_df.columns:
        y_col_age = "age_avg"
    elif "age_mean" in plot_df.columns:
        y_col_age = "age_mean"
    
    if y_col_age is not None:
        smooth_col = f"{y_col_age}_smooth"
        plot_df[smooth_col] = plot_df[y_col_age].rolling(window=window, min_periods=1, center=True).mean()
        fig.add_trace(
            go.Scatter(x=plot_df["tick"], y=plot_df[smooth_col], mode='lines+markers',
                       line=dict(width=4, color="rgba(100, 149, 237, 0.3)"), marker=dict(size=3),
                       name="Age", showlegend=False, hovertemplate='Tick: %{x}<br>Age: %{y:.2f}<extra></extra>'),
            row=1, col=3
        )
        fig.add_trace(
            go.Scatter(x=plot_df["tick"], y=plot_df[smooth_col], mode='lines+markers',
                       line=dict(width=1, color="blue"), marker=dict(size=4, color="blue"),
                       name="Age", showlegend=False, hovertemplate='Tick: %{x}<br>Age: %{y:.2f}<extra></extra>'),
            row=1, col=3
        )
        fig.update_yaxes(title_text="Age", row=1, col=3, range=[0, None])
    
    fig.update_layout(height=300, template="plotly_white", showlegend=False, hovermode='closest')
    fig.update_xaxes(title_text="Tick", showgrid=False, row=1, col=1)
    fig.update_xaxes(title_text="Tick", showgrid=False, row=1, col=2)
    fig.update_xaxes(title_text="Tick", showgrid=False, row=1, col=3)
    
    # Add event vertical lines to all three subplots
    if df_events is not None and len(df_events) > 0:
        # Get y-axis max values for each subplot
        if y_col_health is not None:
            health_smooth = f"{y_col_health}_smooth"
            health_max = plot_df[health_smooth].max() if health_smooth in plot_df.columns else 100
            add_events_to_figure(fig, df_events, y_min=0, y_max=health_max, row=1, col=1)
        if y_col_food is not None:
            food_smooth = f"{y_col_food}_smooth"
            food_max = plot_df[food_smooth].max() if food_smooth in plot_df.columns else 100
            add_events_to_figure(fig, df_events, y_min=0, y_max=food_max, row=1, col=2)
        if y_col_age is not None:
            age_smooth = f"{y_col_age}_smooth"
            age_max = plot_df[age_smooth].max() if age_smooth in plot_df.columns else 100
            add_events_to_figure(fig, df_events, y_min=0, y_max=age_max, row=1, col=3)
    
    return fig


def create_traits_chart(df: pd.DataFrame, df_events: Optional[pd.DataFrame] = None) -> go.Figure:
    """Create creature size, kill ratio, and move ratio chart."""
    plot_df = df.sort_values("tick").copy()
    
    fig = make_subplots(
        rows=1, cols=3,
        subplot_titles=("Creature size average", "Kill ratio", "Move ratio"),
        horizontal_spacing=0.08
    )
    
    window = 5
    
    # Creature size
    y_col_creature = None
    if "creature_size_avg" in plot_df.columns:
        y_col_creature = "creature_size_avg"
    elif "creature_size_mean" in plot_df.columns:
        y_col_creature = "creature_size_mean"
    
    if y_col_creature is not None:
        smooth_col = f"{y_col_creature}_smooth"
        plot_df[smooth_col] = plot_df[y_col_creature].rolling(window=window, min_periods=1, center=True).mean()
        fig.add_trace(
            go.Scatter(x=plot_df["tick"], y=plot_df[smooth_col], mode='lines+markers',
                       line=dict(width=4, color="rgba(128, 128, 128, 0.3)"), marker=dict(size=3),
                       name="Creature size", showlegend=False, hovertemplate='Tick: %{x}<br>Size: %{y:.2f}<extra></extra>'),
            row=1, col=1
        )
        fig.add_trace(
            go.Scatter(x=plot_df["tick"], y=plot_df[smooth_col], mode='lines+markers',
                       line=dict(width=1), marker=dict(size=4),
                       name="Creature size", showlegend=False, hovertemplate='Tick: %{x}<br>Size: %{y:.2f}<extra></extra>'),
            row=1, col=1
        )
        fig.update_yaxes(title_text="Value", row=1, col=1, range=[0, None])
    
    # Kill ratio
    if "can_kill_true_fraction" in plot_df.columns:
        kill_pct = plot_df["can_kill_true_fraction"] * 100
        fig.add_trace(
            go.Scatter(x=plot_df["tick"], y=kill_pct, fill='tozeroy', mode='lines+markers',
                       line=dict(width=0), fillcolor='rgba(255, 182, 193, 0.4)',
                       marker=dict(size=3), name="True", showlegend=False,
                       hovertemplate='Tick: %{x}<br>Kill ratio: %{y:.2f}%<extra></extra>'),
            row=1, col=2
        )
        fig.add_trace(
            go.Scatter(x=plot_df["tick"], y=[100] * len(plot_df), fill='tonexty', mode='lines',
                       line=dict(width=0), fillcolor='rgba(144, 238, 144, 0.4)',
                       name="False", showlegend=False, hoverinfo='skip'),
            row=1, col=2
        )
        fig.add_trace(
            go.Scatter(x=plot_df["tick"], y=kill_pct, mode='lines+markers',
                       line=dict(width=1), marker=dict(size=4),
                       name="Kill ratio", showlegend=False,
                       hovertemplate='Tick: %{x}<br>Kill ratio: %{y:.2f}%<extra></extra>'),
            row=1, col=2
        )
        fig.update_yaxes(title_text="Kill ratio (%)", row=1, col=2, range=[0, 100])
    
    # Move ratio
    if "can_move_true_fraction" in plot_df.columns:
        move_pct = plot_df["can_move_true_fraction"] * 100
        fig.add_trace(
            go.Scatter(x=plot_df["tick"], y=move_pct, fill='tozeroy', mode='lines+markers',
                       line=dict(width=0), fillcolor='rgba(255, 182, 193, 0.4)',
                       marker=dict(size=3), name="True", showlegend=False,
                       hovertemplate='Tick: %{x}<br>Move ratio: %{y:.2f}%<extra></extra>'),
            row=1, col=3
        )
        fig.add_trace(
            go.Scatter(x=plot_df["tick"], y=[100] * len(plot_df), fill='tonexty', mode='lines',
                       line=dict(width=0), fillcolor='rgba(144, 238, 144, 0.4)',
                       name="False", showlegend=False, hoverinfo='skip'),
            row=1, col=3
        )
        fig.add_trace(
            go.Scatter(x=plot_df["tick"], y=move_pct, mode='lines+markers',
                       line=dict(width=1), marker=dict(size=4),
                       name="Move ratio", showlegend=False,
                       hovertemplate='Tick: %{x}<br>Move ratio: %{y:.2f}%<extra></extra>'),
            row=1, col=3
        )
        fig.update_yaxes(title_text="Move ratio (%)", row=1, col=3, range=[0, 100])
    
    fig.update_layout(height=300, template="plotly_white", showlegend=False, hovermode='closest')
    fig.update_xaxes(title_text="Tick", showgrid=False, row=1, col=1)
    fig.update_xaxes(title_text="Tick", showgrid=False, row=1, col=2)
    fig.update_xaxes(title_text="Tick", showgrid=False, row=1, col=3)
    
    # Add event vertical lines to all three subplots
    if df_events is not None and len(df_events) > 0:
        # For subplots with percentage ranges (0-100), use that range
        add_events_to_figure(fig, df_events, y_min=0, y_max=100, row=1, col=2)  # Kill ratio
        add_events_to_figure(fig, df_events, y_min=0, y_max=100, row=1, col=3)  # Move ratio
        # For creature size, get the max value
        if y_col_creature is not None:
            size_smooth = f"{y_col_creature}_smooth"
            size_max = plot_df[size_smooth].max() if size_smooth in plot_df.columns else 100
            add_events_to_figure(fig, df_events, y_min=0, y_max=size_max, row=1, col=1)
    
    return fig


def create_images_chart(df_images: pd.DataFrame, df_events: Optional[pd.DataFrame] = None) -> go.Figure:
    """Create images timeline chart with clickable markers."""
    if df_images is None or len(df_images) == 0:
        fig = go.Figure()
        fig.add_annotation(
            text="No images data available",
            xref="paper", yref="paper",
            x=0.5, y=0.5,
            showarrow=False
        )
        fig.update_layout(
            title="Images Timeline",
            height=300,
            template="plotly_white"
        )
        return fig
    
    plot_df = df_images.sort_values("tick").copy()
    
    # Create a simple line chart showing when images exist
    # Use y=1 for all points to create a horizontal line
    fig = go.Figure()
    
    # Store both tick and file_name in customdata for click handling
    customdata = [[tick, file_name] for tick, file_name in zip(plot_df["tick"], plot_df["file_name"])]
    
    fig.add_trace(go.Scatter(
        x=plot_df["tick"],
        y=[1] * len(plot_df),
        mode='lines+markers',
        line=dict(width=2, color='purple'),
        marker=dict(size=10, color='purple', symbol='square', line=dict(width=2, color='white')),
        name='Images',
        showlegend=False,
        hovertemplate='<b>Image Available</b><br>Tick: %{x}<br>File: %{customdata[1]}<br><i>Click to view</i><extra></extra>',
        customdata=customdata,
    ))
    
    # Add event vertical lines
    if df_events is not None and len(df_events) > 0:
        add_events_to_figure(fig, df_events, y_min=0, y_max=1)
    
    fig.update_layout(
        title="Images Timeline (Click on a point to view the image)",
        xaxis_title="Tick",
        yaxis_title="",
        height=300,
        yaxis_range=[0, 1.2],
        yaxis=dict(showticklabels=False, showgrid=False),
        xaxis=dict(showgrid=False),
        template="plotly_white",
        hovermode='closest',
        clickmode='event+select'
    )
    
    return fig


def get_image_path(colony_id: str, file_name: str) -> Optional[Path]:
    """Get the local path to an image file.
    
    First checks output/bi/<colony_id>/images/, then falls back to
    output/s3/distributed-colony/<colony_id>/images_shots/
    """
    # First check the copied images directory
    colony_path = ANALYTICS_DIR / colony_id / "images" / file_name
    if colony_path.exists():
        return colony_path
    
    # Fall back to original location
    image_path = LOCAL_S3_DIR / colony_id / "images_shots" / file_name
    if image_path.exists():
        return image_path
    
    return None


def show_image_modal(img: Image.Image, tick: int, file_name: str, modal_key: str) -> None:
    """Display an image in a modal popup using custom HTML/CSS."""
    # Convert image to base64 for embedding
    buffered = BytesIO()
    img.save(buffered, format="PNG")
    img_str = base64.b64encode(buffered.getvalue()).decode()
    
    # Create modal HTML/CSS/JavaScript
    modal_html = f"""
    <style>
    .image-modal {{
        display: block;
        position: fixed;
        z-index: 1000;
        left: 0;
        top: 0;
        width: 100%;
        height: 100%;
        overflow: auto;
        background-color: rgba(0, 0, 0, 0.9);
        animation: fadeIn 0.3s;
    }}
    @keyframes fadeIn {{
        from {{ opacity: 0; }}
        to {{ opacity: 1; }}
    }}
    .image-modal-content {{
        margin: auto;
        display: block;
        width: 90%;
        max-width: 90%;
        max-height: 90vh;
        margin-top: 5vh;
        animation: zoomIn 0.3s;
        object-fit: contain;
    }}
    @keyframes zoomIn {{
        from {{ transform: scale(0.8); opacity: 0; }}
        to {{ transform: scale(1); opacity: 1; }}
    }}
    .image-modal-close {{
        position: absolute;
        top: 20px;
        right: 35px;
        color: #f1f1f1;
        font-size: 40px;
        font-weight: bold;
        cursor: pointer;
        z-index: 1001;
        line-height: 1;
        user-select: none;
    }}
    .image-modal-close:hover {{
        color: #bbb;
    }}
    .image-modal-header {{
        position: absolute;
        top: 20px;
        left: 35px;
        color: #f1f1f1;
        font-size: 20px;
        font-weight: bold;
        z-index: 1001;
        background-color: rgba(0, 0, 0, 0.5);
        padding: 10px 15px;
        border-radius: 5px;
    }}
    </style>
    
    <div id="imageModal_{tick}" class="image-modal">
        <span class="image-modal-close" id="closeBtn_{tick}">&times;</span>
        <div class="image-modal-header">Tick {tick} - {file_name}</div>
        <img class="image-modal-content" src="data:image/png;base64,{img_str}" alt="Colony Image at Tick {tick}" style="width: auto; height: auto; max-width: 90%; max-height: 90vh; object-fit: contain;">
    </div>
    
    <script>
        (function() {{
            var modal = document.getElementById('imageModal_{tick}');
            if (!modal) return;
            
            var closeBtn = document.getElementById('closeBtn_{tick}');
            
            function closeModal() {{
                modal.style.display = 'none';
                // Trigger the close function if available
                if (typeof window.closeModal_{tick} === 'function') {{
                    window.closeModal_{tick}();
                }}
            }}
            
            // Close button click
            if (closeBtn) {{
                closeBtn.onclick = function(e) {{
                    e.stopPropagation();
                    closeModal();
                }};
            }}
            
            // Close modal when clicking outside the image
            modal.onclick = function(event) {{
                if (event.target === this) {{
                    closeModal();
                }}
            }};
            
            // Close modal with Escape key
            function handleEscape(event) {{
                if (event.key === 'Escape' && modal.style.display !== 'none') {{
                    closeModal();
                }}
            }}
            document.addEventListener('keydown', handleEscape);
        }})();
    </script>
    """
    
    # Inject JavaScript function first (before modal HTML that uses it)
    trigger_script = f"""
    <script>
        (function() {{
            window.closeModal_{tick} = function() {{
                // Find the Close button by looking for button with text "Close"
                var buttons = Array.from(document.querySelectorAll('button'));
                var closeBtn = buttons.find(function(btn) {{
                    var text = (btn.textContent || btn.innerText || '').trim();
                    return text === 'Close';
                }});
                if (closeBtn) {{
                    closeBtn.click();
                }}
            }};
        }})();
    </script>
    """
    st.markdown(trigger_script, unsafe_allow_html=True)
    
    # Add a close button that updates session state
    if st.button("Close", key=f"close_modal_{tick}", use_container_width=True):
        st.session_state[modal_key] = False
        st.rerun()
    
    # Display the modal HTML (which uses the function defined above)
    st.markdown(modal_html, unsafe_allow_html=True)


def main():
    st.set_page_config(
        page_title="Distributed Colony Analytics",
        page_icon="📊",
        layout="wide",
        initial_sidebar_state="expanded"
    )
    
    # Discover colonies
    colonies = discover_colonies()
    
    if not colonies:
        st.error(f"No colony directories with stats.parquet found under {ANALYTICS_DIR}. Run ingest_bi.py first.")
        return
    
    # Sidebar: Colony selection
    colony_names = [name for name, _ in colonies]
    selected_colony_name = st.sidebar.selectbox(
        "Select Colony",
        colony_names,
        index=0
    )
    
    # Find selected colony path
    selected_colony_path = next(path for name, path in colonies if name == selected_colony_name)
    
    # Load data
    df_stats, df_events, df_images = load_colony_data(selected_colony_path)
    colony_id = get_colony_id(df_stats, selected_colony_path)
    
    # Sidebar: Show events checkbox
    show_events = st.sidebar.checkbox("Show events in charts", value=True)
    
    # Set title with colony ID
    st.title(f"Distributed Colony Dashboard - {colony_id}")
        
    # Prepare events for charts (if checkbox is checked)
    events_for_charts = df_events if show_events else None
    
    # 1. Color Distribution
    fig_color = create_color_chart(df_stats, events_for_charts)
    st.plotly_chart(
        fig_color, 
        use_container_width=True, 
        key="color"
    )
    
    # 2. Creature Coverage
    fig_coverage = create_creature_coverage_chart(df_stats, events_for_charts)
    st.plotly_chart(
        fig_coverage, 
        use_container_width=True, 
        key="coverage"
    )
    
    # Note: Click events are handled via Plotly's interactive hover tooltips
    # For detailed point information, hover over any data point in the chart
    
    # 3. Health, Food, Age
    fig_health = create_health_food_age_chart(df_stats, events_for_charts)
    st.plotly_chart(
        fig_health, 
        use_container_width=True, 
        key="health"
    )
    
    # Metrics details are shown in hover tooltips
    
    # 4. Creature Traits
    fig_traits = create_traits_chart(df_stats, events_for_charts)
    st.plotly_chart(
        fig_traits, 
        use_container_width=True, 
        key="traits"
    )
    
    # Traits details are shown in hover tooltips
    
    # 5. Images Timeline
    if df_images is not None and len(df_images) > 0:
        st.subheader("Images Timeline")
        fig_images = create_images_chart(df_images, events_for_charts)
        
        # Initialize session state for selected image
        if 'selected_image_tick' not in st.session_state:
            st.session_state.selected_image_tick = None
            st.session_state.selected_image_file = None
        
        # Display chart - Streamlit will handle click events via selection
        selected_data = st.plotly_chart(
            fig_images,
            use_container_width=True,
            key="images"
        )
        
        # Handle image selection from chart click
        # Check if user selected a point in the chart
        if selected_data and hasattr(selected_data, 'selection') and selected_data.selection:
            points = selected_data.selection.points if hasattr(selected_data.selection, 'points') else []
            if points and len(points) > 0:
                # Get the first selected point
                point = points[0]
                # Extract tick and file_name from customdata
                if hasattr(point, 'customdata') and point.customdata:
                    tick = point.customdata[0] if isinstance(point.customdata, list) and len(point.customdata) > 0 else None
                    file_name = point.customdata[1] if isinstance(point.customdata, list) and len(point.customdata) > 1 else None
                    if tick is not None and file_name:
                        st.session_state.selected_image_tick = tick
                        st.session_state.selected_image_file = file_name
        
        # Also provide a dropdown to select images by tick (always available)
        st.subheader("Select Image by Tick")
        sorted_images = df_images.sort_values("tick")
        image_options = [f"Tick {row['tick']} - {row['file_name']}" for _, row in sorted_images.iterrows()]
        image_values = [(row['tick'], row['file_name']) for _, row in sorted_images.iterrows()]
        
        selected_option = st.selectbox(
            "Choose an image to view:",
            options=range(len(image_options)),
            format_func=lambda x: image_options[x],
            key="image_selector"
        )
        
        # Determine which image to display (from dropdown or chart click)
        display_tick = None
        display_file = None
        
        if selected_option is not None:
            display_tick, display_file = image_values[selected_option]
        elif st.session_state.selected_image_tick is not None and st.session_state.selected_image_file:
            display_tick = st.session_state.selected_image_tick
            display_file = st.session_state.selected_image_file
        
        # Display the selected image in a popup/modal
        if display_tick is not None and display_file:
            image_path = get_image_path(colony_id, display_file)
            if image_path and image_path.exists():
                try:
                    img = Image.open(image_path)
                    
                    # Initialize modal state (default to False - don't auto-open)
                    modal_key = f"show_modal_{display_tick}_{display_file}"
                    if modal_key not in st.session_state:
                        st.session_state[modal_key] = False
                    
                    # Show button to open modal
                    if st.button(f"View Image at Tick {display_tick}", use_container_width=True, key=f"btn_{display_tick}"):
                        st.session_state[modal_key] = True
                        st.rerun()
                    
                    # Display modal only if state is True (button was clicked)
                    if st.session_state.get(modal_key, False):
                        show_image_modal(img, display_tick, display_file, modal_key)
                except Exception as e:
                    st.error(f"Error loading image: {e}")
            else:
                st.warning(f"Image file not found: {display_file}")
    else:
        st.info("No images data available. Run ingest_bi.py to generate images.parquet")
    
    # 6. Events Timeline (last section)
    if df_events is not None and len(df_events) > 0:
        st.subheader("Events Timeline")
        fig_events = create_events_chart(df_stats, df_events)
        if fig_events is not None:
            st.plotly_chart(
                fig_events, 
                use_container_width=True, 
                key="events"
            )
            
            # Event details are shown in hover tooltips
            
            # Display events table
            events_table_df = prepare_events_table(df_events)
            st.dataframe(
                events_table_df,
                use_container_width=False,
                hide_index=True,
                height=400
            )
    else:
        st.info("No events data available. Run ingest_bi.py to generate events.parquet")
    


if __name__ == "__main__":
    main()

