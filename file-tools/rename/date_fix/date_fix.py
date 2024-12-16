import os
import re
from datetime import datetime
import calendar

def extract_date(filename):
    # Dictionary for month names to numbers
    month_names = {month.lower(): str(i).zfill(2) 
                  for i, month in enumerate(calendar.month_name[1:], 1)}
    
    # Various date patterns to match
    patterns = [
        # YYYY-MM-DD
        r'(\d{4})-(\d{1,2})-(\d{1,2})',
        # DD-MM-YYYY
        r'(\d{1,2})-(\d{1,2})-(\d{4})',
        # MM/DD/YYYY
        r'(\d{1,2})/(\d{1,2})/(\d{4})',
        # Month DD YYYY
        r'([A-Za-z]+)\s+(\d{1,2})\s+(\d{4})',
        # DD Month YYYY
        r'(\d{1,2})\s+([A-Za-z]+)\s+(\d{4})',
        # YYYYMMDD
        r'(\d{4})(\d{2})(\d{2})'
    ]
    
    for pattern in patterns:
        match = re.search(pattern, filename)
        if match:
            groups = match.groups()
            
            # Handle different date formats
            if len(groups[0]) == 4:  # If first group is year
                year, month, day = groups
            elif len(groups[2]) == 4:  # If last group is year
                if any(month.lower() in month_names for month in groups):  # If month name exists
                    for i, group in enumerate(groups):
                        if group.lower() in month_names:
                            month = group
                            day = groups[(i+1)%3]
                            year = groups[2]
                            break
                else:
                    day, month, year = groups
            
            # Convert month names to numbers if necessary
            if isinstance(month, str) and month.lower() in month_names:
                month = month_names[month.lower()]
            
            # Ensure proper formatting
            try:
                date_obj = datetime(int(year), int(month), int(day))
                return date_obj.strftime('%Y-%m-%d'), match.group()
            except ValueError:
                continue
    
    return None, None

def scan_directory(directory):
    changes = {}
    
    for item in os.listdir(directory):
        path = os.path.join(directory, item)
        new_date, old_date = extract_date(item)
        
        if new_date and old_date != new_date:
            new_name = item.replace(old_date, new_date)
            changes[path] = os.path.join(directory, new_name)
    
    return changes

def main():
    # Get directory from user
    directory = input("Enter the directory path to scan: ").strip()
    
    if not os.path.isdir(directory):
        print("Invalid directory path!")
        return
    
    # Scan directory and get proposed changes
    changes = scan_directory(directory)
    
    if not changes:
        print("No files or directories found that need date format changes.")
        return
    
    # Show preview of changes
    print("\nProposed changes:")
    for old_path, new_path in changes.items():
        print(f"'{os.path.basename(old_path)}' -> '{os.path.basename(new_path)}'")
    
    # Ask for confirmation
    confirm = input("\nDo you want to apply these changes? (y/n): ").lower()
    
    if confirm == 'y':
        # Apply changes
        for old_path, new_path in changes.items():
            try:
                os.rename(old_path, new_path)
                print(f"Renamed: {os.path.basename(old_path)} -> {os.path.basename(new_path)}")
            except Exception as e:
                print(f"Error renaming {old_path}: {e}")
        print("\nAll changes applied successfully!")
    else:
        print("\nNo changes were made.")

if __name__ == "__main__":
    main()