# Recuperador de Datos con IA

Un programa ligero para recuperar datos borrados de discos duros, USB y tarjetas SD en Windows, utilizando IA para búsqueda profunda y clasificación de archivos.

## Características
- Escaneo de sectores crudos a bajo nivel
- Clasificación automática de tipos de archivo
- Predicción de recuperabilidad
- Interfaz gráfica simple
- Recuperación de archivos JPEG y PNG

## Requisitos
- Windows
- Python 3.7+
- Bibliotecas: pywin32, PyQt5

## Instalación
1. Instala Python.
2. Ejecuta `pip install -r requirements.txt`
3. Ejecuta `python main.py`

## Uso
1. Ejecuta como administrador (requerido para acceso a disco).
2. Selecciona la unidad física.
3. Selecciona tipos de archivo a buscar.
4. Haz clic en "Escanear" para buscar archivos.
5. Selecciona archivos y "Recuperar Seleccionados" (se organizan por tipo).

## Compilación a EXE
Instala PyInstaller: `pip install pyinstaller`
Ejecuta: `pyinstaller --onefile main.py`

## Advertencias
- Solo lectura, no modifica discos.
- Usa con precaución, puede ser lento en discos grandes.
- No garantiza recuperación completa.

## Licencia
Libre para uso personal.