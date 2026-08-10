# martysama0134' script for packing all txt protos at once
import os
import shutil
import subprocess

# List of folders
folders = ["ae", "cz", "de", "dk", "en", "es", "fr", "gr", "hu", "it", "nl", "pl", "pt", "ro", "ru", "tr"]

# Path to the source files
source_path = r'.'

# Path to the destination folders
destination_base_path = r'_out'

# File names to copy
file_names = ['item_proto.txt', 'mob_proto.txt']

out_file_names = ['item_proto', 'mob_proto']

# Copy and replace files in each folder
for folder in folders:
    translate_folder = os.path.join(source_path, folder)

    # Create the destination folder if it doesn't exist
    # os.makedirs(translate_folder, exist_ok=True)

    # Copy and replace files
    for file_name in file_names:
        source_file_path = os.path.join(source_path, file_name)
        destination_file_path = os.path.join(translate_folder, file_name)

        shutil.copyfile(source_file_path, destination_file_path)

        print(f"Copied {file_name} to {translate_folder}")

    # Run the command inside each folder #remove stdout/stderr redirected to pipe to watch the output message
    command = "..\\dumpproto.exe -pmi"
    subprocess.run(command, shell=True, cwd=translate_folder, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    print(f"Ran DumpProto in {translate_folder}")

    # Create the destination folder if it doesn't exist
    destination_folder = os.path.join(destination_base_path, folder)
    os.makedirs(destination_folder, exist_ok=True)

    # Move and replace files
    for file_name in out_file_names:
        translate_file_path = os.path.join(translate_folder, file_name)
        destination_file_path = os.path.join(destination_folder, file_name)
        if os.path.exists(destination_file_path):
            os.remove(destination_file_path)
        shutil.move(translate_file_path, destination_file_path)

        print(f"Moved {file_name} to {destination_folder}")

    # Clean up
    for file_name in file_names:
        destination_file_path = os.path.join(translate_folder, file_name)
        if os.path.exists(destination_file_path):
            os.remove(destination_file_path)
