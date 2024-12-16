import os
import re
from datetime import datetime
import calendar

def extract_date(filename):
    # Dictionary for month names to numbers (including 3-letter abbreviations)
    month_names = {month.lower(): str(i).zfill(2) 
                  for i, month in enumerate(calendar.month_name[1:], 1)}
    # Add 3-letter abbreviations
    month_names.update({month.lower()[:3]: str(i).zfill(2) 
                       for i, month in enumerate(calendar.month_name[1:], 1)})
    
    # Various date patterns to match
    patterns = [
        # YYYY-MM-DD
        r'(\d{4})-(\d{1,2})-(\d{1,2})',
        # DD-MM-YYYY
        r'(\d{1,2})-(\d{1,2})-(\d{4})',
        # MM/DD/YYYY
        r'(\d{1,2})/(\d{1,2})/(\d{4})',
        # Month DD YYYY or DD Month YYYY (full month name or 3-letter)
        r'(\d{1,2})\s*([A-Za-z]+)\s*(\d{4})',
        r'([A-Za-z]+)\s*(\d{1,2})\s*(\d{4})',
        # YYYYMMDD
        r'(\d{4})(\d{2})(\d{2})',
        # DDMonYY or DDMonYYYY (no spaces)
        r'(\d{1,2})([A-Za-z]{3,})(\d{2,4})',
        # MonDDYY or MonDDYYYY (no spaces)
        r'([A-Za-z]{3,})(\d{1,2})(\d{2,4})'
    ]
    
    for pattern in patterns:
        match = re.search(pattern, filename)
        if match:
            groups = match.groups()
            
            try:
                # Handle different date formats
                if len(groups[0]) == 4:  # If first group is year
                    year, month, day = groups
                elif len(groups[2]) == 4:  # If last group is year
                    if any(group.lower() in month_names for group in groups):  # If month name exists
                        for i, group in enumerate(groups):
                            if group.lower() in month_names:
                                month = group
                                day = groups[(i+1)%3] if i == 0 else groups[(i-1)%3]
                                year = groups[2]
                                break
                    else:
                        day, month, year = groups
                else:  # Handle 2-digit year
                    for i, group in enumerate(groups):
                        if group.lower() in month_names:
                            month = group
                            if len(groups[2]) == 2:  # 2-digit year
                                year = '20' + groups[2] if int(groups[2]) < 50 else '19' + groups[2]
                            else:
                                year = groups[2]
                            day = groups[(i+1)%3] if i == 0 else groups[(i-1)%3]
                            break
                    else:
                        day, month, year = groups
                
                # Convert month names to numbers if necessary
                if isinstance(month, str) and month.lower() in month_names:
                    month = month_names[month.lower()]
                
                # Handle 2-digit years if not handled above
                if len(str(year)) == 2:
                    year = '20' + str(year) if int(year) < 50 else '19' + str(year)
                
                # Ensure proper formatting
                date_obj = datetime(int(year), int(month), int(day))
                return date_obj.strftime('%Y-%m-%d'), match.group()
            
            except (ValueError, KeyError):
                continue
    
    return None, None

def scan_directory(directory):
    """Scan directory for files and directories with dates in their names."""
    changes = {}
    
    for item in os.listdir(directory):
        path = os.path.join(directory, item)
        new_date, old_date = extract_date(item)
        
        if new_date and old_date != new_date:
            new_name = item.replace(old_date, new_date)
            changes[path] = os.path.join(directory, new_name)
    
    return changes

def get_next_version_name(path):
    """Generate a new version name for files that already exist."""
    directory = os.path.dirname(path)
    filename = os.path.basename(path)
    name, ext = os.path.splitext(filename)
    
    version = 2
    while os.path.exists(path):
        new_name = f"{name}_{version}{ext}"
        path = os.path.join(directory, new_name)
        version += 1
    
    return path

def handle_rename(old_path, new_path):
    """Handle the renaming process with conflict resolution."""
    if os.path.exists(new_path):
        print(f"\nTarget already exists: {os.path.basename(new_path)}")
        while True:
            choice = input("Would you like to:\n"
                         "1. Keep original name\n"
                         "2. Add version number\n"
                         "Choose (1 or 2): ").strip()
            
            if choice == '1':
                print(f"Keeping original name: {os.path.basename(old_path)}")
                return False
            elif choice == '2':
                versioned_path = get_next_version_name(new_path)
                try:
                    os.rename(old_path, versioned_path)
                    print(f"Renamed: {os.path.basename(old_path)} -> "
                          f"{os.path.basename(versioned_path)}")
                    return True
                except Exception as e:
                    print(f"Error renaming {old_path}: {e}")
                    return False
            else:
                print("Invalid choice. Please enter 1 or 2.")
    else:
        try:
            os.rename(old_path, new_path)
            print(f"Renamed: {os.path.basename(old_path)} -> "
                  f"{os.path.basename(new_path)}")
            return True
        except Exception as e:
            print(f"Error renaming {old_path}: {e}")
            return False

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
        success_count = 0
        for old_path, new_path in changes.items():
            if handle_rename(old_path, new_path):
                success_count += 1
        
        print(f"\nCompleted! Successfully renamed {success_count} out of {len(changes)} items.")
    else:
        print("\nNo changes were made.")

if __name__ == "__main__":
    main()