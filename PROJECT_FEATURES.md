# recoverPill - Características del Proyecto

**recoverPill** es una herramienta avanzada de recuperación de datos de bajo nivel, diseñada para entornos Windows y optimizada para la velocidad y precisión mediante el uso de Rust y técnicas de análisis heurístico.

## 🛠 Especificaciones Técnicas

- **Lenguaje**: Rust (Edición 2021)
- **Interfaz Gráfica**: `egui` / `eframe` (Moderna, ligera y acelerada por hardware)
- **Acceso a Disco**: Integración nativa con `winapi` para lectura directa de sectores.
- **Paralelismo**: Procesamiento multihilo con `rayon`.

## 🧠 Capacidades de Análisis (Motor de IA)

El proyecto utiliza un motor de clasificación avanzado que va más allá de la simple búsqueda de archivos:

### 1. Análisis de Entropía
- Implementa el cálculo de **Entropía de Shannon** para medir la aleatoriedad de los datos.
- Permite detectar automáticamente si un sector contiene:
  - Datos encriptados o comprimidos (Alta entropía).
  - Texto o código fuente (Entropía media).
  - Sectores vacíos o repetitivos (Baja entropía).

### 2. Clasificación Inteligente de Archivos
- El `AIClassifier` combina la detección por firmas (Magic Bytes) con validación estructural.
- **Validación Específica**: El motor verifica la estructura interna de archivos JPEG, PNG, GIF, BMP y PDF para asegurar su integridad antes de la recuperación.

### 3. Predicción de Recuperabilidad
- Genera un informe de confianza para cada archivo encontrado.
- Calcula la probabilidad de éxito basándose en:
  - Integridad de las cabeceras.
  - Análisis de fragmentación.
  - Consistencia de los datos según su tipo.

## 🚀 Funcionalidades de Recuperación

- **Escaneo de Bajo Nivel**: Capacidad para escanear unidades físicas y lógicas saltándose las restricciones del sistema de archivos.
- **File Carving**: Recuperación de archivos borrados basándose en firmas digitales, ideal para discos con sistemas de archivos dañados o formateados.
- **Detección de Unidades**: Identificación automática de discos duros, unidades USB y tarjetas SD.
- **Visualización en Tiempo Real**: Panel de control interactivo que muestra estadísticas de escaneo y previsualización de archivos encontrados.

## 📁 Estructura del Proyecto

- `disk/`: Módulos de acceso a hardware y detección de sistemas de archivos.
- `core/`: Motor principal de escaneo, base de datos de firmas y lógica de recuperación.
- `ai/`: Componentes de análisis de entropía y clasificación inteligente.
- `ui/`: Interfaz de usuario y gestión de estado de la aplicación.

---
*Desarrollado por recoverPill Team - 2026*
