import json

import requests

with open("port.json") as f:
    PORT = json.load(f)["port"]
BASE_URL = f"http://localhost:{PORT}"

print("GAME")
while True:
    print()

    current = requests.get(f"{BASE_URL}/current").json()
    print(current["display_text"])
    print()
    for i, choice in enumerate(current["choices"]):
        print(f"{i + 1}. {choice['display_text']}")
    print()

    if current["game_over"]:
        break

    choice = int(input("Enter your choice: ")) - 1
    choice_id = current["choices"][choice]["id"]
    response = requests.post(f"{BASE_URL}/choose/{choice_id}")
