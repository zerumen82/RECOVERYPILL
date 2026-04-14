#!/usr/bin/env python3
"""Generate a simple capsule/pill icon as .ico file"""

from PIL import Image, ImageDraw

def create_capsule_icon(size=256):
    """Create a capsule/pill icon with the given size"""
    # Create image with transparent background
    img = Image.new('RGBA', (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    
    # Capsule dimensions
    margin = size // 16
    capsule_width = size - 2 * margin
    capsule_height = size - 2 * margin
    corner_radius = capsule_width // 2
    
    # Draw capsule with two halves (blue top, white bottom)
    top_color = (30, 144, 255, 255)  # DodgerBlue
    bottom_color = (255, 255, 255, 255)  # White
    outline_color = (20, 100, 180, 255)  # Darker blue
    
    # Top half (blue)
    draw.pieslice(
        [margin, margin, margin + capsule_width, margin + capsule_height],
        start=180, end=360,
        fill=top_color, outline=outline_color, width=max(1, size // 128)
    )
    draw.rectangle(
        [margin, margin + corner_radius - 1, margin + capsule_width, margin + capsule_height // 2],
        fill=top_color, outline=None
    )
    
    # Bottom half (white)
    draw.pieslice(
        [margin, margin, margin + capsule_width, margin + capsule_height],
        start=0, end=180,
        fill=bottom_color, outline=outline_color, width=max(1, size // 128)
    )
    draw.rectangle(
        [margin, margin + capsule_height // 2, margin + capsule_width, margin + capsule_height - corner_radius + 1],
        fill=bottom_color, outline=None
    )
    
    # Redraw outline
    draw.pieslice(
        [margin, margin, margin + capsule_width, margin + capsule_height],
        start=0, end=360,
        fill=None, outline=outline_color, width=max(2, size // 64)
    )
    
    # Add a line in the middle to separate halves
    mid_y = margin + capsule_height // 2
    draw.line(
        [(margin + 2, mid_y), (margin + capsule_width - 2, mid_y)],
        fill=outline_color, width=max(1, size // 128)
    )
    
    return img

def main():
    # Create multiple sizes for the icon
    sizes = [16, 32, 48, 64, 128, 256]
    images = []
    
    for size in sizes:
        img = create_capsule_icon(size)
        images.append(img)
    
    # Save as .ico with all sizes
    output_path = "capsule.ico"
    images[0].save(
        output_path,
        format='ICO',
        sizes=[(img.width, img.height) for img in images],
        append_images=images[1:]
    )
    print(f"Icon saved to {output_path}")

if __name__ == "__main__":
    main()
