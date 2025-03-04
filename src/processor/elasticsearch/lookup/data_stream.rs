use crate::data::{
    diagnostic::Lookup,
    elasticsearch::{DataStream, DataStreams, Indices},
};
use color_eyre::eyre::Result;

impl From<&String> for Lookup<DataStream> {
    fn from(string: &String) -> Self {
        let data_streams: DataStreams =
            serde_json::from_str(&string).expect("Failed to parse DataStreamData");
        Lookup::<DataStream>::from(data_streams)
    }
}

impl From<DataStreams> for Lookup<DataStream> {
    fn from(mut data_streams: DataStreams) -> Self {
        let mut lookup = Lookup::<DataStream>::new();
        data_streams
            .data_streams
            .drain(..)
            .for_each(|mut data_stream| {
                data_stream.build();
                let name = data_stream.name.clone();
                let mut indices: Indices = data_stream.indices.drain(..).collect();
                let write_index = indices.len() - 1;
                let write_data_stream = data_stream.clone().set_write_index(true);
                if write_index > 0 {
                    lookup.add(data_stream).with_name(&name);
                }

                for (i, index) in indices.drain(..).enumerate() {
                    if i == write_index {
                        lookup
                            .add(write_data_stream.clone())
                            .with_name(&name)
                            .with_id(&index.index_name.clone());
                    } else {
                        lookup.with_id(&index.index_name.clone());
                    }
                }
            });

        log::debug!("lookup data_stream entries: {}", lookup.len(),);
        lookup
    }
}

impl From<Result<DataStreams>> for Lookup<DataStream> {
    fn from(data_streams: Result<DataStreams>) -> Self {
        match data_streams {
            Ok(data_streams) => Lookup::<DataStream>::from(data_streams),
            Err(e) => {
                log::warn!("Failed to parse DataStreams: {}", e);
                Lookup::new()
            }
        }
    }
}
