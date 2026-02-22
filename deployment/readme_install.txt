AuTHRity - Advanced Process Control System
===========================================

Thank you for choosing AuTHRity!

This installer will set up a complete Model Predictive Control (MPC) system on your Windows machine.

WHAT WILL BE INSTALLED:
-----------------------
- APC Engine (DMC Controller)
- Controller Host (REST API)
- OPC UA Server (Process Communication)
- HMI Web Interface
- QuestDB Time-Series Database
- Java Runtime Environment (JRE 11)
- Virtual Plant Simulator (optional)

SYSTEM REQUIREMENTS:
--------------------
- Windows 10 (build 17763+) or Windows 11
- 64-bit processor
- 4 GB RAM minimum (8 GB recommended)
- 2 GB free disk space
- Administrator privileges

NETWORK PORTS USED:
-------------------
- 3000  - HMI Web Interface
- 4840  - OPC UA Server
- 8080  - Controller Host API
- 9000  - QuestDB Console
- 9009  - QuestDB Data Ingestion

AFTER INSTALLATION:
-------------------
The installer will create Windows services for all components.
You can access the HMI at: http://localhost:3000

Services will be registered as:
- AuTHRity-QuestDB
- AuTHRity-OPCServer
- AuTHRity-ControllerHost
- AuTHRity-HMI
- AuTHRity-VirtualPlant (if selected)

All logs are written to: C:\Program Files\AuTHRity\logs\

FIREWALL NOTICE:
----------------
Windows Firewall may prompt you to allow network access for:
- Java (QuestDB)
- authrity-hmi.exe
- authrity-opcua-server.exe
- authrity-controller-host.exe

Please allow access for the applications to function properly.

GETTING STARTED:
----------------
1. Complete the installation
2. Open browser to http://localhost:3000
3. Load a model file
4. Deploy controller
5. Monitor process via HMI

DOCUMENTATION:
--------------
Full documentation is available in:
C:\Program Files\AuTHRity\docs\

For support, visit: https://github.com/yourusername/authrity

UNINSTALLING:
-------------
Use Windows Settings → Apps or Control Panel → Programs
All services will be stopped and removed automatically.

Press "Next" to continue with the installation.
